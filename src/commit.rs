use std::path::Path;

use anyhow::{Context, Result, anyhow};
use git2::{Commit, Oid, Repository};
use rig::{
    agent::AgentBuilder,
    client::{CompletionClient, ProviderClient},
    completion::Prompt,
    providers::gemini,
};

use crate::utils::get_staged_diff;

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

/// Generates a commit message using an AI model.
///
/// This function uses `rig`'s `Agent` to generate a commit message based on the staged git diff,
/// recent commit history, and an optional user-provided prefix.
///
/// # Arguments
/// * `prefix` - An optional prefix for the commit message.
/// * `model` - The name of the AI model to use.
/// * `history_depth` - The number of recent commits to include as context.
/// * `ignored` - A list of file patterns to ignore from commit history.
pub async fn commit_completion(
    prefix: &str,
    model: &str,
    history_depth: &usize,
    ignored: &Option<Vec<String>>,
) -> Result<String> {
    let diff = get_staged_diff().context("Getting staged changes failed")?;
    if diff.trim().is_empty() {
        return Err(anyhow!("Cannot generate commit message from empty diff"));
    }

    let ignored_patterns: Option<Vec<&str>> = ignored
        .as_ref()
        .map(|v| v.iter().map(|s| s.as_str()).collect());
    let history = get_recent_commits(history_depth, ignored_patterns.as_deref())?;

    let history_context = history
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

    let client = gemini::Client::from_env();
    let completion_model = client.completion_model(model);

    let input_prompt = format!(
        "CODE_CHANGES:\n```diff\n{diff}\n```\n\nCOMMIT_HISTORY:\n{history_context}\n\nPARTIAL_COMMIT_MESSAGE: {prefix}",
    );

    let agent = AgentBuilder::new(completion_model)
        .preamble(PREAMBLE)
        .temperature(0.2)
        .max_tokens(200)
        .build();

    let response = agent.prompt(input_prompt).await?;

    Ok(response)
}

/// Retrieves the last N commits from the repository, optionally ignoring commits that match patterns.
pub fn get_recent_commits(
    history_depth: &usize,
    ignore_patterns: Option<&[&str]>,
) -> Result<Vec<(Oid, String)>> {
    let repo = Repository::open(Path::new(".")).context("Opening git repository failed")?;
    let head_commit = repo
        .head()
        .context("Getting repository HEAD failed")?
        .peel_to_commit()
        .context("Getting HEAD commit failed")?;
    let mut revwalk = repo.revwalk().context("Creating revision walker failed")?;
    revwalk
        .push(head_commit.id())
        .context("Setting starting commit failed")?;
    revwalk
        .set_sorting(git2::Sort::TIME)
        .context("Setting sort order failed")?;

    let mut commits = Vec::new();
    for id in revwalk.take(*history_depth) {
        match id {
            Ok(oid) => match repo.find_commit(oid) {
                Ok(commit) => {
                    if !should_ignore_commit(&commit, ignore_patterns) {
                        commits.push((oid, commit.message().unwrap_or_default().to_string()));
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
    Ok(commits)
}

/// Determines if a commit should be ignored based on file patterns.
fn should_ignore_commit(commit: &Commit, ignore_patterns: Option<&[&str]>) -> bool {
    if let Some(patterns) = ignore_patterns {
        if let Ok(tree) = commit.tree() {
            for pattern in patterns {
                if tree.iter().any(|entry| {
                    entry
                        .name()
                        .map(|name| name.contains(pattern))
                        .unwrap_or(false)
                }) {
                    return true;
                }
            }
        }
    }
    false
}
