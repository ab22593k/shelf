// git commit  # Generates message
// git commit -m "msg"  # Uses provided message
// git commit --amend  # Generates message for amended changes
// git commit --squash HEAD~3  # Generates message for squashed changes
// git merge branch  # Uses git's merge message

use anyhow::{anyhow, Result};
use clap::Parser;
use colored::*;
use slf::{
    gitai::{providers::create_provider, utils::git_diff, GitAIConfig},
    spinner,
};

use std::{path::PathBuf, process::exit};

#[derive(Debug, Clone, PartialEq)]
enum Source {
    Message,
    Template,
    Merge,
    Squash,
    Commit,
}

impl std::str::FromStr for Source {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "message" => Ok(Source::Message),
            "template" => Ok(Source::Template),
            "merge" => Ok(Source::Merge),
            "squash" => Ok(Source::Squash),
            "commit" => Ok(Source::Commit),
            other => Err(anyhow!("Invalid source: {}", other)),
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "prepare-commit-msg")]
struct Args {
    #[arg(help = "Path to the commit message file")]
    commit_msg_file: PathBuf,

    #[arg(help = "Source of the commit")]
    source: Option<Source>,

    #[arg(help = "Commit hash if amending")]
    sha1: Option<String>,
}

async fn handle_commit(args: &Args) -> Result<()> {
    // Generate commit message from detailed diff
    let config = GitAIConfig::load().await?;
    let provider = create_provider(&config)?;

    let mut commit_msg = spinner::new(|| async {
        let diff = git_diff();
        provider.generate_commit_message(&diff?).await
    })
    .await?;

    commit_msg = commit_msg.trim().to_string();

    // Write the message
    std::fs::write(&args.commit_msg_file, commit_msg)?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Skip message generation for certain commit types
    if let Some(Source::Message | Source::Template | Source::Merge) = args.source {
        return Ok(());
    }

    // Check if there's already a message
    if args.commit_msg_file.exists() {
        let content = std::fs::read_to_string(&args.commit_msg_file)?;
        if !content.trim().is_empty() {
            return Ok(());
        }
    }

    if let Err(e) = handle_commit(&args).await {
        eprintln!("{} {}", "Error:".red().bold(), e);
        exit(1);
    }

    Ok(())
}
