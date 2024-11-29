pub mod git;
pub mod http;
pub mod prompt;
pub mod provider;

use anyhow::Result;
use async_trait::async_trait;
use colored::Colorize;
use git::{get_diff_cached, install_git_hook, remove_git_hook};
use prompt::PromptKind;
use provider::create_provider;

use crate::{config::ShelfConfig, spinner};

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

pub async fn handle_ai_commit(
    app_conf: ShelfConfig,
    provider_override: Option<String>,
    model_override: Option<String>,
    install_hook: bool,
    remove_hook: bool,
) -> Result<()> {
    let repo = git2::Repository::open_from_env()?;
    let hooks_dir = repo.path().join("hooks");

    if install_hook {
        return install_git_hook(&hooks_dir);
    }

    if remove_hook {
        return remove_git_hook(&hooks_dir);
    }

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
            .generate_assistant_message(PromptKind::Commit, &diff)
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
    configs: ShelfConfig,
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
            .generate_assistant_message(PromptKind::Review, &diff)
            .await
    })
    .await?;

    println!("{}\n{}", "Code review:".green().bold(), review);
    Ok(())
}
