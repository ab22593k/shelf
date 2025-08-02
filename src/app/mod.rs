use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::app::dots::Dots;

pub mod commit;
pub mod completion;
pub mod dots;
pub mod review;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Shelf {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage system configuration files.
    Dots(dots::DotsCommand),
    /// Generate a commit message using AI or manage git hooks.
    Commit(commit::CommitCommand),
    /// Review code changes and suggest improvements using AI.
    Review(review::ReviewCommand),
    /// Generate shell completion scripts.
    Completion(completion::CompletionCommand),
}

pub async fn run_app(cli: Shelf, repo: Dots) -> Result<()> {
    match cli.command {
        Commands::Dots(args) => dots::run(args, repo).await?,
        Commands::Commit(args) => commit::run(args).await?,
        Commands::Review(args) => review::run(args).await?,
        Commands::Completion(args) => completion::run(args).await?,
    }
    Ok(())
}
