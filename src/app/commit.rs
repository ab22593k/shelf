use anyhow::{Context, Result, anyhow};
use clap::Args;
use colored::Colorize;
use handlebars::Handlebars;
use rig::client::builder::DynClientBuilder;
use rig::completion::Prompt;
use rig::providers::gemini::completion::gemini_api_types::{self};
use serde_json::json;
use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

use crate::app::git::{collect_changes, commit_action, commit_history};
use crate::app::ui::{UserAction, user_selection};

const COMMIT_TEMPLATE_PATH: &str = "assets/prompts/commit_message_completion.hbs";
const PREAMBLE_TEMPLATE_PATH: &str = "assets/prompts/assistant_commit_preamble.hbs";

const PROPOSED_HEADER: &str = "Proposed Commit Message:";
const CANCELLED_TEXT: &str = "Operation cancelled.";

const AI_TEMPERATURE: f64 = 0.2;
const AI_MAX_TOKENS: u64 = 200;

const EDITOR_ENV_VARS: [&str; 3] = ["GIT_EDITOR", "EDITOR", "VISUAL"];

#[derive(Args)]
pub struct CommitCommand {
    /// Prefix to prepend to the generated commit message
    #[arg(long, default_value = "")]
    pub prefix: Option<String>,
    /// AI model provider to use for generation
    #[arg(short, long, default_value = "gemini")]
    pub provider: String,
    /// Specific model to use for commit message generation
    #[arg(short, long, default_value = "gemini-2.5-flash-lite")]
    pub model: String,
    /// Number of previous commits to include as context
    #[arg(long, short = 'd', default_value = "10")]
    pub history_depth: usize,
    /// File patterns to ignore when generating commits (comma-separated)
    #[arg(short, long, default_value = None, value_delimiter = ',', num_args = 1..)]
    pub ignored: Option<Vec<String>>,
}

pub async fn run(args: CommitCommand) -> Result<()> {
    let config = CommitConfig::from(&args);
    execute_commit_workflow(&config).await
}

/// Configuration context for commit message generation
struct CommitConfig<'a> {
    prefix: Option<&'a str>,
    provider: &'a str,
    model: &'a str,
    history_depth: &'a usize,
    ignored_patterns: &'a Option<Vec<String>>,
}

impl<'a> From<&'a CommitCommand> for CommitConfig<'a> {
    fn from(cmd: &'a CommitCommand) -> Self {
        Self {
            prefix: cmd.prefix.as_deref(),
            provider: &cmd.provider,
            model: &cmd.model,
            history_depth: &cmd.history_depth,
            ignored_patterns: &cmd.ignored,
        }
    }
}

/// Main commit workflow orchestrator
async fn execute_commit_workflow(config: &CommitConfig<'_>) -> Result<()> {
    let mut commit_message = String::new();

    loop {
        // Generate message only when needed (first time or after regeneration)
        if commit_message.is_empty() {
            commit_message = generate_commit_message(config).await?;
        }

        display_proposed_message(&commit_message);

        match user_selection()? {
            UserAction::RegenerateMessage => {
                commit_message.clear(); // Force regeneration on next iteration
            }
            UserAction::EditWithEditor => {
                commit_message = edit_with_external_editor(&commit_message)?;
            }
            UserAction::CommitChanges => {
                commit_action(commit_message)?;
                return Ok(());
            }
            UserAction::Quit | UserAction::Cancelled => {
                display_cancellation_message();
                return Ok(());
            }
        }
    }
}

/// Generate commit message using AI model
async fn generate_commit_message(config: &CommitConfig<'_>) -> Result<String> {
    let response = request_commit_suggestion(config).await?;

    // Try to parse as Gemini API response first, fall back to raw text
    if let Ok(parsed_response) =
        serde_json::from_str::<gemini_api_types::GenerateContentResponse>(&response)
    {
        Ok(extract_text_from_gemini_response(&parsed_response))
    } else {
        Ok(response)
    }
}

/// Request commit message suggestion from AI model
async fn request_commit_suggestion(config: &CommitConfig<'_>) -> Result<String> {
    let diff_content = collect_changes().context("Failed to retrieve staged changes")?;

    validate_diff_content(&diff_content)?;

    let commit_history = build_commit_history(config)?;
    let rendered_prompt = build_prompt_from_template(config, &diff_content, &commit_history)?;

    let client = create_client(config)?;
    client.prompt(rendered_prompt).await.map_err(|e| anyhow!(e))
}

/// Create and configure AI client for commit message generation
fn create_client(config: &CommitConfig<'_>) -> Result<impl Prompt> {
    let client_builder = DynClientBuilder::new();
    let preamble_content = load_template_with_fallback(PREAMBLE_TEMPLATE_PATH)?;
    let agent = client_builder
        .agent(config.provider, config.model)?
        .preamble(&preamble_content)
        .temperature(AI_TEMPERATURE)
        .max_tokens(AI_MAX_TOKENS)
        .build();

    Ok(agent)
}

/// Extract text content from Gemini API response
fn extract_text_from_gemini_response(
    response: &gemini_api_types::GenerateContentResponse,
) -> String {
    response
        .candidates
        .first()
        .map(|candidate| candidate.content.parts.first())
        .and_then(|part| match &part.part {
            gemini_api_types::PartKind::Text(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "No commit message generated".to_string())
}

/// Build the complete prompt from template and context data
fn build_prompt_from_template(
    config: &CommitConfig<'_>,
    diff_content: &str,
    commit_history: &str,
) -> Result<String> {
    let template_content = load_template_with_fallback(COMMIT_TEMPLATE_PATH)?;
    let template_data = create_template_data(config, diff_content, commit_history);

    let handlebars = Handlebars::new();
    handlebars
        .render_template(&template_content, &template_data)
        .context("Failed to render prompt template")
}

/// Create template data for Handlebars rendering
fn create_template_data(
    config: &CommitConfig<'_>,
    diff_content: &str,
    commit_history: &str,
) -> serde_json::Value {
    let partial_commit_section = config
        .prefix
        .filter(|p| !p.is_empty())
        .map(|p| format!("PARTIAL_COMMIT_MESSAGE:\n```\n{p}```\n"))
        .unwrap_or_default();

    json!({
        "CODE_CHANGES": format!("```diff\n{diff_content}\n```"),
        "COMMIT_HISTORY": format!("```\n{commit_history}\n```\n"),
        "PARTIAL_COMMIT_MESSAGE": partial_commit_section
    })
}

/// Load a template from file, trying the current directory first, then a fallback user config directory.
fn load_template_with_fallback(template_path: &str) -> Result<String> {
    let current_dir_path = std::path::PathBuf::from(template_path);

    // Try reading from the current directory first
    match std::fs::read_to_string(&current_dir_path) {
        Ok(content) => Ok(content),
        Err(e_current) => {
            // If that fails, try the user's config directory
            let home_var = if cfg!(target_os = "windows") {
                "USERPROFILE"
            } else {
                "HOME"
            };
            let home_dir = std::env::var(home_var)
                .with_context(|| format!("{home_var} environment variable not found"))?;

            let config_dir_path = std::path::PathBuf::from(home_dir)
                .join(".config")
                .join("shelf")
                .join(template_path);

            match std::fs::read_to_string(&config_dir_path) {
                Ok(content) => Ok(content),
                Err(e_config) => Err(anyhow!(
                    "Failed to read template from:\n  - Current directory: {:?} ({})\n  - Config directory: {:?} ({})",
                    current_dir_path,
                    e_current,
                    config_dir_path,
                    e_config
                )),
            }
        }
    }
}

/// Build formatted commit history string
fn build_commit_history(config: &CommitConfig<'_>) -> Result<String> {
    let ignored_patterns = config
        .ignored_patterns
        .as_ref()
        .map(|patterns| patterns.iter().map(String::as_str).collect::<Vec<_>>());

    let commits = commit_history(config.history_depth, ignored_patterns.as_deref())?;

    if commits.is_empty() {
        return Ok(String::new());
    }

    let formatted_history = commits
        .iter()
        .map(|(oid, message)| {
            format!(
                "• {}: {}",
                &oid.to_string()[..7],
                message.lines().next().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!("COMMIT_HISTORY:\n{formatted_history}"))
}

/// Validate that there are staged changes to commit
fn validate_diff_content(diff: &str) -> Result<()> {
    if diff.trim().is_empty() {
        Err(anyhow!("Cannot generate commit message from empty diff"))
    } else {
        Ok(())
    }
}

/// Display the proposed commit message to the user
fn display_proposed_message(message: &str) {
    println!("\n{PROPOSED_HEADER}\n{}", message.bright_yellow());
}

/// Display cancellation message
fn display_cancellation_message() {
    println!("{}", CANCELLED_TEXT.bright_blue());
}

/// Open external editor for commit message editing
fn edit_with_external_editor(initial_message: &str) -> Result<String> {
    let temp_file = create_temp_file_with_content(initial_message)?;
    let editor = determine_editor();

    launch_editor(&editor, temp_file.path())?;
    read_edited_content(temp_file.path())
}

/// Create temporary file with initial content
fn create_temp_file_with_content(content: &str) -> Result<NamedTempFile> {
    let mut temp_file = NamedTempFile::new().context("Failed to create temporary file")?;
    temp_file
        .write_all(content.as_bytes())
        .context("Failed to write content to temporary file")?;
    Ok(temp_file)
}

/// Determine which editor to use based on environment variables
fn determine_editor() -> String {
    EDITOR_ENV_VARS
        .iter()
        .find_map(|&key| std::env::var(key).ok())
        .unwrap_or_else(default_editor)
}

/// Get platform-specific default editor
fn default_editor() -> String {
    if cfg!(target_os = "windows") {
        "notepad".to_string()
    } else {
        "vi".to_string()
    }
}

/// Launch the chosen editor with the specified file
fn launch_editor(editor: &str, file_path: &std::path::Path) -> Result<()> {
    let status = Command::new(editor)
        .arg(file_path)
        .status()
        .with_context(|| format!("Failed to launch editor: {editor}"))?;

    if !status.success() {
        return Err(anyhow!("Editor exited with non-zero status"));
    }
    Ok(())
}

/// Read the edited content from the temporary file
fn read_edited_content(file_path: &std::path::Path) -> Result<String> {
    std::fs::read_to_string(file_path).context("Failed to read edited content from temporary file")
}
