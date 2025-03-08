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
    /// Optional footer to add issue tracker context
    pub footer: Option<Vec<String>>,
}

impl std::fmt::Display for CommitMsgContinuation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = if let Some(emoji) = &self.emoji {
            format!("{} ", emoji)
        } else {
            String::new()
        };
        writeln!(f, "{}{}: {}", prefix, self.r#type, self.title)?;

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
            let footer_str = footer
                .iter()
                .map(|issue| issue.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(f, "\n{}", footer_str)?;
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
pub async fn commit_completion(
    prefix: &str,
    model: &str,
    history_depth: &usize,
    ignored: &Option<Vec<String>>,
) -> Result<CommitMsgContinuation> {
    // Exit early if diff is empty
    let diff = get_staged_diff().context("Getting staged changes failed")?;
    if diff.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "Cannot generate commit message from empty diff"
        ));
    }

    let ignored_str_refs: Option<Vec<&str>> = ignored
        .as_ref()
        .map(|v| v.iter().map(|s| s.as_str()).collect());
    let history = get_recent_commits(history_depth, ignored_str_refs.as_deref())?;

    let history_aware = history
        .iter()
        .map(|(oid, message)| format!("Commit Hash: {}\nCommit Message: {}", oid, message))
        .collect::<Vec<_>>()
        .join("\n---\n");

    let client = gemini::Client::from_env();

    // Configure and build the commit extractor with proper context
    let commit_completion = client
        .extractor::<CommitMsgContinuation>(model)
        .preamble(
            r#"Generate a structured commit message from the git diff. Use concise,
                descriptive titles in present tense. Ensure clarity and relevance.

                Consider these guidelines:
                - Start with a clear action verb
                - Keep each line of the body under 80 characters for readability
                - Keep the first line under 50 characters


                You will be provided with:
                1. A commit message prefix that needs to be continued
                2. The git diff of the changes
                3. Recent commit history for context
                "#,
        )
        .build();

    // Complete commit information using the AI model
    let input = format!(
        "Commit message prefix:\n{}\n\nRecent commit history:\n{}\n\n:\n{}",
        prefix, history_aware, diff
    );

    let commit = commit_completion
        .extract(input.as_str())
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
    match id {
        Ok(oid) => match repo.find_commit(oid) {
            Ok(commit) => {
                if should_ignore_commit(&commit, ignore_patterns) {
                    return None;
                }
                Some((oid, commit))
            }
            Err(err) => {
                eprintln!("Failed to find commit {}: {}", oid, err);
                None
            }
        },
        Err(err) => {
            // Log the error for better debugging
            eprintln!("Invalid commit ID: {}", err);
            None
        }
    }
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
