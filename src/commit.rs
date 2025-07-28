use std::path::Path;

use anyhow::{Context, Result, anyhow};
use git2::{Commit, Oid, Repository};
use rig::{
    agent::AgentBuilder,
    client::{CompletionClient, ProviderClient},
    completion::Prompt,
    providers::gemini,
};

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

/// Conjures commit suggestions from the ether.
///
/// This function leverages `rig`'s `Agent` to manifest a commit message by
/// synthesizing staged git diffs, recent commit lore, and an optional user-crafted prefix.
///
/// # Arguments
/// * `commit_prefix` - An optional precursor to the commit message.
/// * `ai_model` - The moniker of the AI model to invoke.
/// * `history_span` - The number of recent commits to weave into the narrative.
/// * `excluded_files` - A list of file patterns to shroud from the commit history.
pub async fn conjure_commit_suggestion(
    commit_prefix: &str,
    ai_model: &str,
    history_span: &usize,
    excluded_files: &Option<Vec<String>>,
) -> Result<String> {
    let diff = harvest_staged_changes().context("Conjuring staged changes failed")?;
    if diff.trim().is_empty() {
        return Err(anyhow!(
            "Cannot conjure a commit message from an empty diff"
        ));
    }

    let ignored_patterns: Option<Vec<&str>> = excluded_files
        .as_ref()
        .map(|v| v.iter().map(|s| s.as_str()).collect());
    let commit_chronicle = fetch_commit_saga(history_span, ignored_patterns.as_deref())?;

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

    let client = gemini::Client::from_env();
    let muse_model = client.completion_model(ai_model);

    let prompt_essence = format!(
        "CODE_CHANGES:\n```diff\n{diff}\n```\n\nCOMMIT_HISTORY:\n{history_tapestry}\n\nPARTIAL_COMMIT_MESSAGE: {commit_prefix}",
    );

    let artificer = AgentBuilder::new(muse_model)
        .preamble(PREAMBLE)
        .temperature(0.2)
        .max_tokens(200)
        .build();

    let revelation = artificer.prompt(prompt_essence).await?;

    Ok(revelation)
}

/// Fetches the last N commits from the repository, optionally evading commits matching patterns.
pub fn fetch_commit_saga(
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
                    if !should_shun_commit(&commit, evade_patterns) {
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

/// Ascertains if a commit should be shunned based on file patterns.
fn should_shun_commit(commit: &Commit, evade_patterns: Option<&[&str]>) -> bool {
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
