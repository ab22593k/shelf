use anyhow::{Context, Result, anyhow};
use clap::Args;
use colored::Colorize;
use git2::{Commit, Oid, Repository};
use rig::client::builder::DynClientBuilder;
use rig::completion::Prompt;
use rig::providers::gemini::completion::gemini_api_types::{self, Part};
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;

use crate::app::ui::{UserAction, user_selection};
use crate::utils::harvest_staged_changes;

const PREAMBLE: &str = r#"You are an expert software developer assistant specialized in crafting clear, concise, informative, and contextually relevant Git commit messages. Your primary task is to **complete a given partial commit message**. You will be provided with a summary of the current code changes and relevant past commit history to help you understand the context and maintain a consistent style and 'personal' nature.

The goal is to produce high-quality, complete commit messages that effectively track changes and aid collaboration. Ensure the completed message clearly summarizes the change, its purpose, and integrates seamlessly with the partial message provided. **Only return the completed commit message, do not add any additional conversational text or explanations**.

---

**EXAMPLE 1 (Few-shot)**

**CODE_CHANGES:**
```diff
-  def old_auth_method():
+  def new_secure_auth_method():
```
**COMMIT_HISTORY:**
• feat: Implement user authentication module
• refactor: Refactor database schema for better performance
• fix: Resolve critical security vulnerability in login flow
**PARTIAL_COMMIT_MESSAGE:** refactor: Rename old_auth_method to new_
**COMPLETED_COMMIT_MESSAGE:** refactor: Rename old_auth_method to new_secure_auth_method for enhanced security and clarity"#;

/// A helper struct to group common parameters used across commit generation functions.
struct CommitContext<'a> {
    prefix: &'a str,
    provider: &'a str,
    model: &'a str,
    history_depth: &'a usize,
    ignored: &'a Option<Vec<String>>,
}

impl<'a> From<&'a CommitCommand> for CommitContext<'a> {
    fn from(args: &'a CommitCommand) -> Self {
        CommitContext {
            prefix: &args.prefix,
            provider: &args.provider,
            model: &args.model,
            history_depth: &args.history_depth,
            ignored: &args.ignored,
        }
    }
}

#[derive(Args)]
pub struct CommitCommand {
    /// Suitable continuation context for the commit message.
    #[arg(long)]
    pub prefix: String,
    /// Override the model provider.
    #[arg(short, long, default_value = "gemini")]
    pub provider: String,
    /// Override the configured model.
    #[arg(short, long, default_value = "gemini-2.0-flash-lite")]
    pub model: String,
    /// Include the nth commit history.
    #[arg(long, short = 'd', default_value = "10")]
    pub history_depth: usize,
    /// Ignore specific files or patterns when generating a commit (comma-separated).
    #[arg(short, long, default_value = None, value_delimiter = ',', num_args = 1..)]
    pub ignored: Option<Vec<String>>,
}

pub async fn run(args: CommitCommand) -> Result<()> {
    let context = CommitContext::from(&args);
    handle_commit_action(&context).await?;
    Ok(())
}

async fn generate_commit_message(context: &CommitContext<'_>) -> Result<String> {
    let response = commit_suggestion(context).await?;

    // If the response is a Gemini JSON structure, extract the commit message; otherwise, use the plain string
    let commit_msg = if let Ok(parsed) =
        serde_json::from_str::<gemini_api_types::GenerateContentResponse>(&response)
    {
        commit_message_res(&parsed)
    } else {
        response
    };
    Ok(commit_msg)
}

async fn handle_commit_action(context: &CommitContext<'_>) -> Result<()> {
    let mut current_commit_msg = String::new();

    loop {
        if current_commit_msg.is_empty() {
            // Generate commit message using AI model only if not already generated or edited
            current_commit_msg = generate_commit_message(context).await?;
        }

        println!(
            "\nProposed Commit Message:\n{}",
            current_commit_msg.bright_yellow()
        );

        // Get user action selection
        let selection = user_selection()?;
        match selection {
            UserAction::RegenerateMessage => {
                // Clear to force regeneration
                current_commit_msg.clear();
                continue;
            }
            UserAction::EditWithEditor => {
                current_commit_msg = edit_message_with_editor(&current_commit_msg)
                    .context("Failed to edit commit message with editor")?;
                // After editing, show the new message and prompt again
                continue;
            }
            UserAction::CommitChanges => {
                commit_action(current_commit_msg)?;
                return Ok(()); // Commit successful, exit the loop and function
            }
            UserAction::Quit | UserAction::Cancelled => {
                println!("{}", "Operation cancelled or quit.".bright_blue());
                return Ok(()); // User decided to quit or cancelled the prompt, exit successfully
            }
        }
    }
}

fn edit_message_with_editor(initial_message: &str) -> Result<String> {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().context("Failed to create temporary file")?;

    // Write the initial message to the temporary file
    std::io::Write::write_all(&mut temp_file, initial_message.as_bytes())
        .context("Failed to write initial message to temporary file")?;

    // Determine the editor to use
    let editor = std::env::var("GIT_EDITOR")
        .or_else(|_| std::env::var("EDITOR"))
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "windows") {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        });

    // Spawn the editor process
    let status = Command::new(&editor)
        .arg(temp_file.path())
        .status()
        .with_context(|| format!("Failed to open editor: {editor}",))?;

    if !status.success() {
        return Err(anyhow!("Editor exited with a non-zero status."));
    }

    // Read the modified content from the temporary file
    let edited_message = std::fs::read_to_string(temp_file.path())
        .context("Failed to read edited message from temporary file")?;

    Ok(edited_message)
}

fn commit_message_res(response: &gemini_api_types::GenerateContentResponse) -> String {
    response
        .candidates
        .first()
        .and_then(|candidate| candidate.content.parts.iter().next())
        .and_then(|part| match part {
            Part::Text(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| String::from("No commit message generated"))
}

fn commit_action(message: String) -> Result<String> {
    let repo = Repository::open(".")?;
    let signature = repo.signature()?;
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    let parents = match repo.head() {
        Ok(head) => vec![head.peel_to_commit()?],
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => vec![],
        Err(e) => return Err(e.into()),
    };

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        parents.iter().collect::<Vec<_>>().as_slice(),
    )?;

    println!("{}", "Created git commit successfully".bright_green());
    Ok(message)
}

async fn commit_suggestion(context: &CommitContext<'_>) -> Result<String> {
    let diff = harvest_staged_changes().context("Conjuring staged changes failed")?;
    if diff.trim().is_empty() {
        return Err(anyhow!(
            "Cannot conjure a commit message from an empty diff"
        ));
    }

    let ignored_patterns: Option<Vec<&str>> = context
        .ignored
        .as_ref()
        .map(|v| v.iter().map(|s| s.as_str()).collect());
    let commit_chronicle = commit_history(context.history_depth, ignored_patterns.as_deref())?;

    let history_tapestry = commit_chronicle
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

    let client_builder = DynClientBuilder::new();
    let agent = client_builder
        .agent(context.provider, context.model)? // Propagate error instead of unwrap()
        .preamble(PREAMBLE)
        .temperature(0.2)
        .max_tokens(200)
        .build();

    let prompt = format!(
        "CODE_CHANGES:\n```diff\n{diff}\n```\n\nCOMMIT_HISTORY:\n{history_tapestry}\n\nPARTIAL_COMMIT_MESSAGE: {}",
        context.prefix,
    );

    let revelation = agent.prompt(prompt).await?;

    Ok(revelation)
}

pub fn commit_history(
    saga_depth: &usize,
    evade_patterns: Option<&[&str]>,
) -> Result<Vec<(Oid, String)>> {
    let repository =
        Repository::open(Path::new(".")).context("Unearthing git repository failed")?;
    let apex_commit = repository
        .head()
        .context("Seeking repository HEAD failed")?
        .peel_to_commit()
        .context("Seeking HEAD commit failed")?;
    let mut chronicle_walker = repository
        .revwalk()
        .context("Weaving revision chronicle failed")?;
    chronicle_walker
        .push(apex_commit.id())
        .context("Anchoring starting commit failed")?;
    chronicle_walker
        .set_sorting(git2::Sort::TIME)
        .context("Setting chronicle order failed")?;

    let mut saga_chapters = Vec::new();
    for commit_id in chronicle_walker.take(*saga_depth) {
        match commit_id {
            Ok(oid) => match repository.find_commit(oid) {
                Ok(commit) => {
                    if !evade_commit(&commit, evade_patterns) {
                        saga_chapters.push((oid, commit.message().unwrap_or_default().to_string()));
                    }
                }
                Err(err) => {
                    eprintln!("Failed to find commit {oid}: {err}");
                }
            },
            Err(err) => {
                eprintln!("Invalid commit ID: {err}");
            }
        }
    }
    Ok(saga_chapters)
}

fn evade_commit(commit: &Commit, evade_patterns: Option<&[&str]>) -> bool {
    if let (Some(patterns), Ok(file_tree)) = (evade_patterns, commit.tree()) {
        for pattern in patterns {
            if file_tree.iter().any(|entry| {
                entry
                    .name()
                    .map(|name| name.contains(pattern))
                    .unwrap_or(false)
            }) {
                return true;
            }
        }
    }
    false
}
