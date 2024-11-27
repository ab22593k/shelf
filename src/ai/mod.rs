pub mod git;
pub mod http;
pub mod prompt;
pub mod provider;

use anyhow::Result;
use async_trait::async_trait;
use prompt::PromptKind;

/// Trait for AI providers.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Generate a response from the AI model.
    async fn generate_assistant_message(&self, prompt: PromptKind, diff: &str) -> Result<String>;

    /// Format the user prompt with the diff.
    fn format_prompt(&self, prompt: &str, diff: &str) -> String {
        format!("{}\n{}", prompt, diff)
    }
}
