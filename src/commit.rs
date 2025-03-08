use anyhow::Result;
use rig::providers::gemini;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
/// A record representing a git commit message
pub struct Commit {
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
}

impl std::fmt::Display for Commit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}: {}", self.r#type, self.title)?;
        // Write the emoji and title side by side
        // if let Some(emoji) = &self.emoji {
        //     write!(f, "{} ", emoji)?;
        // }

        // Add the body with proper spacing if it exists
        if let Some(body) = &self.body {
            writeln!(f, "\n{}", body)?;
        }

        // Add breaking changes with proper spacing if it exists
        if let Some(breaking_changes) = &self.breaking_changes {
            writeln!(f, "\nBREAKING CHANGES:\n{}", breaking_changes)?;
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
#[instrument(skip(client), fields(diff_len = git_diff.len(), history_len = commit_history.len()))]
pub async fn extract_commit_from_diff(
    client: &gemini::Client,
    git_diff: &str,
    commit_history: &[String],
) -> Result<Commit> {
    // Exit early if diff is empty
    if git_diff.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "Cannot generate commit message from empty diff"
        ));
    }

    // Configure and build the commit extractor with proper context
    let commit_extractor = client
        .extractor::<Commit>("gemini-2.0-flash-lite")
        .preamble(
            r#"
            Generate a structured commit message from the git diff. Use concise,
            descriptive titles in present tense. Ensure clarity and relevance.

            Consider these guidelines:
            - Start with a clear action verb
            - Keep each line of the body under 80 characters for readability
            - Keep the first line under 50 characters
            "#,
        )
        .context(&commit_history.join("\n---\n")) // Improved separator for better readability
        .build();

    // Extract commit information using the AI model
    let commit = commit_extractor
        .extract(&git_diff)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to extract commit data: {}", e))?;

    // Log the extracted commit for debugging purposes
    if let Ok(json) = serde_json::to_string_pretty(&commit) {
        println!("Extracted Commit Data:\n{}", json);
    }

    Ok(commit)
}
