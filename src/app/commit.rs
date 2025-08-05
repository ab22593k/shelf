use anyhow::{Context, Result, anyhow};
use clap::Args;
use colored::Colorize;
use handlebars::Handlebars;
use rig::client::builder::DynClientBuilder;
use rig::completion::Prompt;
use rig::providers::gemini::completion::gemini_api_types::{self, Part};
use serde_json::json;
use std::process::Command;
use tempfile::NamedTempFile;

use crate::app::git::{commit_action, commit_history};
use crate::app::ui::{UserAction, user_selection};
use crate::utils::harvest_staged_changes;

/// Configuration context for commit message generation
struct CommitConfig<'a> {
    prefix: &'a str,
    provider: &'a str,
    model: &'a str,
    history_depth: &'a usize,
    ignored_patterns: &'a Option<Vec<String>>,
}

impl<'a> From<&'a CommitCommand> for CommitConfig<'a> {
    fn from(cmd: &'a CommitCommand) -> Self {
        Self {
            prefix: &cmd.prefix,
            provider: &cmd.provider,
            model: &cmd.model,
            history_depth: &cmd.history_depth,
            ignored_patterns: &cmd.ignored,
        }
    }
}

#[derive(Args)]
pub struct CommitCommand {
    /// Prefix to prepend to the generated commit message
    #[arg(long, default_value = "")]
    pub prefix: String,

    /// AI model provider to use for generation
    #[arg(short, long, default_value = "gemini")]
    pub provider: String,

    /// Specific model to use for commit message generation
    #[arg(short, long, default_value = "gemini-2.0-flash-lite")]
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

/// Main commit workflow orchestrator
async fn execute_commit_workflow(config: &CommitConfig<'_>) -> Result<()> {
    let mut commit_message = String::new();

    loop {
        // Generate message only when needed (first time or after regeneration)
        if commit_message.is_empty() {
            commit_message = generate_commit_message(config).await?;
        }

        display_proposed_message(&commit_message);

        match get_user_action()? {
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

/// Display the proposed commit message to the user
fn display_proposed_message(message: &str) {
    println!("\nProposed Commit Message:\n{}", message.bright_yellow());
}

/// Get user's chosen action
fn get_user_action() -> Result<UserAction> {
    user_selection()
}

/// Display cancellation message
fn display_cancellation_message() {
    println!("{}", "Operation cancelled.".bright_blue());
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

    std::io::Write::write_all(&mut temp_file, content.as_bytes())
        .context("Failed to write content to temporary file")?;

    Ok(temp_file)
}

/// Determine which editor to use based on environment variables
fn determine_editor() -> String {
    std::env::var("GIT_EDITOR")
        .or_else(|_| std::env::var("EDITOR"))
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| default_editor())
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

/// Extract text content from Gemini API response
fn extract_text_from_gemini_response(
    response: &gemini_api_types::GenerateContentResponse,
) -> String {
    response
        .candidates
        .first()
        .map(|candidate| candidate.content.parts.first())
        .and_then(|part| match part {
            Part::Text(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "No commit message generated".to_string())
}

/// Request commit message suggestion from AI model
async fn request_commit_suggestion(config: &CommitConfig<'_>) -> Result<String> {
    let diff_content = harvest_staged_changes().context("Failed to retrieve staged changes")?;

    validate_diff_content(&diff_content)?;

    let commit_history = build_commit_history(config)?;
    let rendered_prompt = build_prompt_from_template(config, &diff_content, &commit_history)?;

    let client = create_client(config)?;
    let response = client.prompt(rendered_prompt).await?;

    Ok(response)
}

/// Validate that there are staged changes to commit
fn validate_diff_content(diff: &str) -> Result<()> {
    if diff.trim().is_empty() {
        return Err(anyhow!("Cannot generate commit message from empty diff"));
    }
    Ok(())
}

/// Build formatted commit history string
fn build_commit_history(config: &CommitConfig<'_>) -> Result<String> {
    let ignored_patterns = config
        .ignored_patterns
        .as_ref()
        .map(|patterns| patterns.iter().map(String::as_str).collect::<Vec<_>>());

    let commits = commit_history(config.history_depth, ignored_patterns.as_deref())?;

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

    if formatted_history.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("COMMIT_HISTORY:\n{formatted_history}"))
    }
}

/// Build the complete prompt from template and context data
fn build_prompt_from_template(
    config: &CommitConfig<'_>,
    diff_content: &str,
    commit_history: &str,
) -> Result<String> {
    let template_content = load_prompt_template()?;
    let template_data = create_template_data(config, diff_content, commit_history);

    let handlebars = Handlebars::new();
    handlebars
        .render_template(&template_content, &template_data)
        .context("Failed to render prompt template")
}

/// Load the Handlebars prompt template from file
fn load_prompt_template() -> Result<String> {
    std::fs::read_to_string("assets/assistant_commit_prompt.hbs")
        .context("Failed to read commit prompt template")
}

/// Create template data for Handlebars rendering
fn create_template_data(
    config: &CommitConfig<'_>,
    diff_content: &str,
    commit_history: &str,
) -> serde_json::Value {
    let partial_commit_section = if config.prefix.is_empty() {
        String::new()
    } else {
        format!("PARTIAL_COMMIT_MESSAGE:\n```\n{}```\n", config.prefix)
    };

    json!({
        "CODE_CHANGES": format!("```diff\n{diff_content}\n```"),
        "COMMIT_HISTORY": format!("```\n{commit_history}\n```\n"),
        "PARTIAL_COMMIT_MESSAGE": partial_commit_section
    })
}

/// Create and configure AI client for commit message generation
fn create_client(config: &CommitConfig<'_>) -> Result<impl Prompt> {
    let client_builder = DynClientBuilder::new();
    let agent = client_builder
        .agent(config.provider, config.model)?
        .temperature(0.2)
        .max_tokens(200)
        .build();

    Ok(agent)
}
