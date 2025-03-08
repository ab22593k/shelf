use std::path::Path;

use anyhow::{Context, Result};
use git2::Repository;
use rig::providers::gemini;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::utils::get_staged_diff;

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
/// A record representing a git commit message
pub struct CommitMsgContinuation {
    /// The type of commit
    pub r#type: String,
    /// The commit title/summary
    pub title: String,
    /// The commit description/body
    pub body: Option<String>,
    /// Optional breaking changes introduced by the commit
    pub breaking_changes: Option<String>,
    /// Optional emoji to prepend to the commit title
    pub emoji: Option<String>,
    /// Optional footer to add additional context
    pub footer: Option<String>,
}

impl std::fmt::Display for CommitMsgContinuation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}: {}", self.r#type, self.title)?;

        // Add the body with proper spacing if it exists
        if let Some(body) = &self.body {
            writeln!(f, "\n{}", body)?;
        }

        // Add breaking changes with proper spacing if it exists
        if let Some(breaking_changes) = &self.breaking_changes {
            writeln!(f, "\nBREAKING CHANGES:\n{}", breaking_changes)?;
        }

        // Add footer with proper spacing if it exists
        if let Some(footer) = &self.footer {
            writeln!(f, "\n{}", footer)?;
        }

        Ok(())
    }
}

/// Extracts commit data from the git diff using Gemini AI
///
/// # Arguments
/// * `client` - The Gemini client instance
/// * `git_diff` - The git diff content to analyze
/// * `commit_history` - Previous commit messages for context
///
/// # Returns
/// * `Result<Commit>` - The extracted commit data or an error
/// * `r#ref` - Optional issue references to include in the commit message context
pub async fn extract_commit_from_diff(
    prefix: &str,
    model: &str,
    history_depth: &usize,
    r#ref: &Option<Vec<usize>>,
) -> Result<CommitMsgContinuation> {
    // Exit early if diff is empty
    let diff = get_staged_diff().context("Getting staged changes failed")?;
    if diff.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "Cannot generate commit message from empty diff"
        ));
    }

    //  let pp = prefix.insert_str(
    //      0,
    //      " given the prefix of a commit message and
    // the commit context, generate a suitable continuation for the
    // commit message. ",
    //  );

    let history = get_recent_commits(history_depth, None)?;

    let history_context = history
        .iter()
        .map(|(oid, message)| format!("Commit Hash: {}\nCommit Message: {}", oid, message))
        .collect::<Vec<_>>()
        .join("\n---\n");

    let mut context_parts = vec![history_context];

    if let Some(refs) = r#ref {
        if !refs.is_empty() {
            let issue_context = format!(
                "Issue References: {}\nThe following issue references are related to the changes in the diff and should be considered when generating the commit message. ",
                refs.iter()
                    .map(|r| format!("#{}", r))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            context_parts.push(issue_context);
        }
    }

    let full_context = context_parts.join("\n---\n");

    let client = gemini::Client::from_env();

    // Configure and build the commit extractor with proper context
    let commit_extractor = client
        .extractor::<CommitMsgContinuation>(model)
        .preamble(
            r#"Generate a structured commit message from the git diff. Use concise,
            descriptive titles in present tense. Ensure clarity and relevance.

            Consider these guidelines:
            - Start with a clear action verb
            - Keep each line of the body under 80 characters for readability
            - Keep the first line under 50 characters
            - If issue references are provided, consider them to understand the context and purpose of the changes.
            "#,
        )
        .context(format!(" given the prefix of a commit message and
       the commit context, generate a suitable continuation for the
       commit message: {}", prefix).as_str())
        .context(&full_context)
        .build();

    // Extract commit information using the AI model
    let commit = commit_extractor
        .extract(&diff)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to extract commit data: {}", e))?;

    // Log the extracted commit for debugging purposes
    if let Ok(json) = serde_json::to_string_pretty(&commit) {
        println!("Extracted Commit Data:\n{}", json);
    }

    Ok(commit)
}

/// Get the last Nth commits from the repository
pub fn get_recent_commits(
    history: &usize,
    ignore_patterns: Option<&[&str]>,
) -> Result<Vec<(git2::Oid, String)>> {
    let repo = Repository::open(Path::new(".")).context("Opening git repository failed")?;
    let head_commit = get_head_commit(&repo)?;
    let revwalk = setup_revision_walker(&repo, &head_commit)?;

    Ok(revwalk
        .take(*history)
        .filter_map(|id| {
            process_commit(&repo, id, ignore_patterns)
                .map(|(oid, commit)| (oid, commit.message().unwrap_or_default().to_string()))
        })
        .collect())
}

fn get_head_commit(repo: &Repository) -> Result<git2::Commit> {
    repo.head()
        .context("Getting repository HEAD failed")?
        .peel_to_commit()
        .context("Getting HEAD commit failed")
}

fn setup_revision_walker<'a>(
    repo: &'a Repository,
    head_commit: &git2::Commit,
) -> Result<git2::Revwalk<'a>> {
    let mut revwalk = repo.revwalk().context("Creating revision walker failed")?;
    revwalk
        .push(head_commit.id())
        .context("Setting starting commit failed")?;
    revwalk
        .set_sorting(git2::Sort::TIME)
        .context("Setting sort order failed")?;

    Ok(revwalk)
}

fn process_commit<'repo>(
    repo: &'repo Repository,
    id: Result<git2::Oid, git2::Error>,
    ignore_patterns: Option<&[&str]>,
) -> Option<(git2::Oid, git2::Commit<'repo>)> {
    id.ok().and_then(|id| {
        let commit = repo.find_commit(id).ok()?;
        if should_ignore_commit(&commit, ignore_patterns) {
            return None;
        }

        Some((id, commit))
    })
}

fn should_ignore_commit(commit: &git2::Commit, ignore_patterns: Option<&[&str]>) -> bool {
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
