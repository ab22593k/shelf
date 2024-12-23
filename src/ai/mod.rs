pub mod git;
pub mod http;
pub mod prompt;
pub mod provider;

use anyhow::Result;
use async_trait::async_trait;
use colored::Colorize;
use git::get_diff_cached;
use prompt::SysPromptKind;
use provider::create_provider;

use crate::{config::Config, spinner};

/// Trait for AI providers.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Generate a response from the AI model.
    async fn generate_assistant_message(&self, prompt: SysPromptKind, diff: &str)
        -> Result<String>;

    /// Format the user prompt with the diff.
    fn format_prompt(&self, prompt: &str, diff: &str) -> String {
        format!("{}\n{}", prompt, diff)
    }
}

pub async fn handle_ai_commit(
    app_conf: Config,
    provider_override: Option<String>,
    model_override: Option<String>,
) -> Result<()> {
    // let mut config = AI::load().await?;
    let mut ai_config = app_conf.read_all()?;
    if let Some(provider_name) = provider_override {
        ai_config.provider = provider_name;
    }
    if let Some(model_name) = model_override {
        ai_config.model = model_name;
    }

    let provider = create_provider(&ai_config)?;
    let commit_msg = spinner::new(|| async {
        let diff = get_diff_cached(".")?;
        provider
            .generate_assistant_message(SysPromptKind::Commit, &diff)
            .await
    })
    .await?;

    println!(
        "{}\n{}",
        "Generated commit message:".green().bold(),
        commit_msg
    );
    Ok(())
}

pub async fn handle_ai_review(
    configs: Config,
    provider_override: Option<String>,
    model_override: Option<String>,
) -> Result<()> {
    let mut dd = configs.read_all()?;
    if let Some(provider_name) = provider_override {
        dd.provider = provider_name;
    }
    if let Some(model_name) = model_override {
        dd.model = model_name;
    }

    let provider = create_provider(&dd)?;
    let review = spinner::new(|| async {
        let diff = get_diff_cached(".")?;
        provider
            .generate_assistant_message(SysPromptKind::Review, &diff)
            .await
    })
    .await?;

    println!("{}\n{}", "Code review:".green().bold(), review);
    Ok(())
}
