// git commit  # Generates message
// git commit -m "msg"  # Uses provided message
// git commit --amend  # Generates message for amended changes
// git commit --squash HEAD~3  # Generates message for squashed changes
// git merge branch  # Uses git's merge message

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use colored::*;
use git2::{Oid, Repository};
use slf::gitai::GitAI;
use std::path::PathBuf;
use std::process::exit;

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

async fn handle_commit(args: &Args, repo: &Repository) -> Result<()> {
    // Get the changes to commit
    let diff = match args.sha1.as_deref() {
        Some(sha1) if sha1.starts_with("HEAD~") => {
            let n = sha1[5..]
                .parse::<usize>()
                .context("Failed to parse N from HEAD~N")?;
            let mut commit = repo.head()?.peel_to_commit()?;
            for _ in 0..n {
                commit = commit.parent(0)?;
            }
            let tree = commit.tree()?;
            repo.diff_tree_to_index(Some(&tree), None, None)?
        }
        Some("HEAD") | None => repo.diff_index_to_workdir(None, None)?,
        Some(sha1) => {
            let obj = repo.find_object(Oid::from_str(sha1)?, None)?;
            let tree = obj.peel_to_tree()?;
            repo.diff_tree_to_index(Some(&tree), None, None)?
        }
    };

    if !diff.stats()?.files_changed() == 0 {
        bail!("No changes to commit");
    }

    // Generate a detailed diff text with file changes
    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_, _, line| {
        let origin = line.origin();
        let content = std::str::from_utf8(line.content()).unwrap_or("");

        match origin {
            '+' => diff_text.push_str(&format!("+{}", content)),
            '-' => diff_text.push_str(&format!("-{}", content)),
            'F' => diff_text.push_str(&format!("\nFile: {}\n", content)),
            'H' => diff_text.push_str(&format!("{}", content)),
            _ => diff_text.push_str(&format!(" {}", content)),
        }
        true
    })?;

    // Generate commit message from detailed diff
    let gitai = GitAI::new(None).await?;
    let mut commit_msg = gitai.generate_commit_message(&diff_text).await?;

    commit_msg = commit_msg.trim().to_string();

    // Write the message
    std::fs::write(&args.commit_msg_file, commit_msg)?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Skip message generation for certain commit types
    match args.source {
        Some(Source::Message | Source::Template | Source::Merge) => {
            return Ok(());
        }
        _ => {}
    }

    // Check if there's already a message
    if args.commit_msg_file.exists() {
        let content = std::fs::read_to_string(&args.commit_msg_file)?;
        if !content.trim().is_empty() {
            return Ok(());
        }
    }

    let repo = Repository::open_from_env().context("Failed to open git repository")?;

    if let Err(e) = handle_commit(&args, &repo).await {
        eprintln!("{} {}", "Error:".red().bold(), e);
        exit(1);
    }

    Ok(())
}
