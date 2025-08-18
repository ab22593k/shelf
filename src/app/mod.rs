pub(super) mod dots;

mod commit;
mod completion;
mod prompt;
mod review;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::app::dots::Dots;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Shelf {
    #[command(subcommand)]
    pub command: Claps,
}

#[derive(Subcommand)]
pub enum Claps {
    /// Manage system configuration files.
    Dots(dots::DotsCMD),
    /// Generate a commit message using AI or manage git hooks.
    Commit(commit::CommitCMD),
    /// Review code changes and suggest improvements using AI.
    Review(review::ReviewCMD),
    /// Text based prompt and repository context.
    Prompt(prompt::PromptCMD),
    /// Generate shell completion scripts.
    Completion(completion::CompletionCMD),
}

pub async fn run_app(cli: Shelf, repo: Dots) -> Result<()> {
    match cli.command {
        Claps::Dots(args) => dots::run(args, repo).await?,
        Claps::Commit(args) => commit::run(args).await?,
        Claps::Review(args) => review::run(args).await?,
        Claps::Prompt(args) => prompt::run(args).await?,
        Claps::Completion(args) => completion::run(args).await?,
    }
    Ok(())
}
