use crate::{app::git::collect_changes, utils::spin_progress};
use anyhow::{Context, Result, anyhow};
use clap::Args;
use handlebars::Handlebars;
use rig::{client::builder::DynClientBuilder, completion::Prompt};
use serde_json::json;
use std::path::Path;

const REVIEW_TEMPLATE_PATH: &str = "assets/prompts/comprehensive_review.hbs";
const AI_TEMPERATURE: f64 = 0.2;

const PREAMBLE: &str = r"You are a **Senior Rust Software Architect** and an **expert Code Reviewer**. Your primary mission is to meticulously analyze provided Git diffs (code changes) for software quality, security, and adherence to best practices, then offer highly actionable and insightful feedback.";

#[derive(Args)]
pub struct ReviewCommand {
    /// Override the configured provider.
    #[arg(short, long, default_value = "gemini")]
    pub provider: String,
    /// Override the configured model.
    #[arg(short, long, default_value = "gemini-2.5-flash")]
    pub model: String,
}

pub(super) async fn run(args: ReviewCommand) -> Result<()> {
    let reviews = review_action(args).await?;
    println!("{reviews}");
    Ok(())
}

/// Orchestrates the code review process.
async fn review_action(args: ReviewCommand) -> Result<String> {
    let diff = collect_changes().context("Failed to get staged changes")?;
    let template = load_review_template()?;
    let prompt = build_review_prompt(&template, &diff)?;
    request_review(prompt, &args).await
}

/// Loads the review prompt template from the primary path or a fallback configuration directory.
fn load_review_template() -> Result<String> {
    let primary_path = Path::new(REVIEW_TEMPLATE_PATH);

    std::fs::read_to_string(primary_path).or_else(|e1| {
        let base_dirs = directories::BaseDirs::new()
            .ok_or_else(|| anyhow!("Could not find home directory to construct fallback path"))?;

        let mut fallback_path = base_dirs.config_dir().to_path_buf();
        fallback_path.push("shelf");
        fallback_path.push(REVIEW_TEMPLATE_PATH);

        std::fs::read_to_string(&fallback_path).map_err(|e2| {
            anyhow!(
                "Failed to read review prompt template from:\n- Primary path: {} (Error: {})\n- Fallback path: {} (Error: {})",
                primary_path.display(),
                e1,
                fallback_path.display(),
                e2
            )
        })
    })
    .context("Failed to load review prompt template")
}

/// Renders the main prompt using the provided template and code changes.
fn build_review_prompt(template_str: &str, diff: &str) -> Result<String> {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(handlebars::no_escape);

    let data = json!({ "CODE_CHANGES": diff });

    handlebars
        .render_template(template_str, &data)
        .context("Failed to render review prompt template")
}

/// Requests a code review from the AI agent.
async fn request_review(prompt: String, args: &ReviewCommand) -> Result<String> {
    let client = DynClientBuilder::new();
    let agent = client
        .agent(&args.provider, &args.model)?
        .preamble(PREAMBLE)
        .temperature(AI_TEMPERATURE)
        .build();

    spin_progress(|| async { agent.prompt(prompt).await.map_err(anyhow::Error::from) }).await
}
