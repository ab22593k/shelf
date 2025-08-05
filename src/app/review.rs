use crate::utils::{harvest_staged_changes, spin_progress};
use anyhow::{Context, Result, anyhow};
use clap::Args;
use handlebars::Handlebars;
use rig::{client::builder::DynClientBuilder, completion::Prompt};
use serde_json::json;

#[derive(Args)]
pub struct ReviewCommand {
    /// Override the configured provider.
    #[arg(short, long, default_value = "gemini")]
    pub provider: String,
    /// Override the configured model.
    #[arg(short, long, default_value = "gemini-2.0-flash")]
    pub model: String,
}

pub(super) async fn run(args: ReviewCommand) -> Result<()> {
    // Pass the entire args struct to review_action
    let reviews = review_action(args).await?;
    println!("{reviews}");
    Ok(())
}

async fn review_action(args: ReviewCommand) -> Result<String> {
    let mut reg = Handlebars::new();
    reg.register_escape_fn(handlebars::no_escape);

    let diff = harvest_staged_changes().context("Getting staged changes failed")?;

    let review_template_str = std::fs::read_to_string("assets/assistant_review_prompt.hbs")
        .context("Failed to read review prompt template")?;
    let review_data = json!({ "CODE_CHANGES": &diff });
    let main_prompt = reg.render_template(&review_template_str, &review_data)?;

    let client = DynClientBuilder::new();
    let agent = client
        .agent(args.provider.as_str(), args.model.as_str())?
        .preamble(&std::fs::read_to_string(
            "assets/assistant_review_preamble.hbs",
        )?)
        .temperature(0.2)
        .build();

    let msg = spin_progress(|| async {
        let reviews = agent.prompt(main_prompt).await;

        reviews.map_err(|e| anyhow!(e))
    })
    .await?;

    Ok(msg)
}
