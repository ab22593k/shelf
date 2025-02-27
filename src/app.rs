use anyhow::{Context, Result, anyhow};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use colored::Colorize;
use git2::Repository;
use rig::{completion::Prompt, providers::gemini};
use std::{
    collections,
    path::{Path, PathBuf},
};

use crate::{
    commit::MsgCompletion,
    review::Reviewer,
    utils::{get_staged_diff, run_with_progress},
};
use crate::{
    dotfs::{DotFs, ListFilter},
    shell::completions_script,
};

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
    Dotfs {
        #[command(subcommand)]
        action: FileAction,
    },
    /// Generate a commit message using AI or manage git hooks.
    Commit {
        /// Override the configured model.
        #[arg(short, long, default_value = "gemini-2.0-flash-lite")]
        model: String,
        /// Add issue reference to the commit footer.
        #[arg(short, long, default_value = None)]
        fixes: Option<usize>,
        /// Include the nth commit history.
        #[arg(long, default_value = "10")]
        history: usize,
    },
    /// Review code changes and suggest improvements using AI.
    Review {
        /// Override the configured model.
        #[arg(short, long, default_value = "gemini-2.0-flash-thinking-exp-01-21")]
        model: String,
    },
    /// Generate shell completion scripts.
    Completion {
        /// The shell to generate completions for.
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand)]
pub enum FileAction {
    /// Track files for management.
    Track {
        /// Paths to the files to track.
        paths: Vec<PathBuf>,
    },
    /// Remove files from management.
    Untrack {
        /// Paths to the files to untrack.
        paths: Vec<PathBuf>,
    },
    /// List all currently tracked files.
    List {
        /// List only modified files.
        #[arg(short, long)]
        dirty: bool,
    },
    /// Pin files for management.
    Save,
}

pub async fn run_app(cli: Shelf, repo: DotFs) -> Result<()> {
    match &cli.command {
        Commands::Dotfs { action } => {
            handle_files_command(action, repo).await?;
        }
        Commands::Commit {
            model,
            history,
            fixes,
        } => {
            handle_commit_action(model.as_str(), fixes, history).await?;
        }
        Commands::Review { model } => {
            let msg = handle_review_action(model.as_str()).await?;
            println!("{}", msg);
        }
        Commands::Completion { shell } => {
            let mut cmd = Shelf::command();
            let script =
                completions_script(*shell, &mut cmd).context("Printing completions failed")?;
            println!("{}", script);
        }
    }
    Ok(())
}

async fn handle_files_command(action: &FileAction, mut repo: DotFs) -> Result<()> {
    match action {
        FileAction::Track { paths } => handle_file_tracking(paths, &mut repo)?,
        FileAction::Untrack { paths } => handle_file_untracking(paths, &mut repo)?,
        FileAction::List { dirty } => display_files(repo, *dirty)?,
        FileAction::Save => handle_file_saving(&mut repo)?,
    }
    Ok(())
}

fn handle_file_saving(repo: &mut DotFs) -> Result<()> {
    repo.save_changes().context("Pinning tabs failed")?;
    println!("{}", "Tabs pinned successfully".bright_green());
    Ok(())
}

fn handle_file_tracking(paths: &[PathBuf], repo: &mut DotFs) -> Result<()> {
    repo.track(paths).context("Tracking files failed")?;
    for path in paths {
        println!("Tracking {}", path.display().to_string().bright_green());
    }
    Ok(())
}

fn handle_file_untracking(paths: &[PathBuf], repo: &mut DotFs) -> Result<()> {
    repo.untrack(paths).context("Untracking files failed")?;
    for path in paths {
        println!("Untracking {}", path.display().to_string().bright_red());
    }
    Ok(())
}

fn display_files(mut repo: DotFs, dirty: bool) -> Result<()> {
    let paths = match dirty {
        true => {
            repo.set_filter(ListFilter::Modified);
            repo.collect()
        }
        false => repo.collect(),
    };
    let paths_by_dir = group_tabs_by_directory(paths);

    for (dir, files) in paths_by_dir {
        if !dir.as_os_str().is_empty() {
            println!("{}:", dir.display().to_string().blue().bold());
        }
        for file in files {
            println!(
                "  {}",
                file.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .bright_green()
            );
        }
    }
    Ok(())
}

fn group_tabs_by_directory(paths: Vec<PathBuf>) -> collections::BTreeMap<PathBuf, Vec<PathBuf>> {
    let mut paths_by_dir: collections::BTreeMap<PathBuf, Vec<PathBuf>> =
        collections::BTreeMap::new();
    for file in paths {
        let parent = file.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
        paths_by_dir.entry(parent).or_default().push(file);
    }

    paths_by_dir
}

async fn handle_commit_action(
    model: &str,
    fixes: &Option<usize>,
    history: &usize,
) -> Result<String> {
    // Initialize client and get required git info
    let diff = get_staged_diff().context("Getting staged changes failed")?;
    let commit_history = get_recent_commits(Path::new("."), history, None).unwrap_or_default(); // Handle first commit case by using empty history if get_recent_commits fails

    loop {
        // Generate commit message using AI
        let msg = generate_commit_message(model, fixes, &commit_history, &diff).await?;
        println!("{}", msg);

        // Get user action selection
        let selection = user_selection()?;
        match selection {
            UserAction::RegenerateMessage => continue,
            UserAction::CommitChanges => return create_git_commit(&msg),
            UserAction::Quit => return Ok("Quitting".to_string()),
            UserAction::Cancelled => return Ok("Cancelled".to_string()),
        }
    }
}

enum UserAction {
    RegenerateMessage,
    CommitChanges,
    Quit,
    Cancelled,
}

fn user_selection() -> Result<UserAction> {
    use dialoguer::{Select, theme::ColorfulTheme};
    let options = vec!["Regenerate message", "Commit changes", "Quit"];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do next?")
        .default(0)
        .items(&options)
        .interact();

    match selection {
        Ok(0) => Ok(UserAction::RegenerateMessage),
        Ok(1) => Ok(UserAction::CommitChanges),
        Ok(2) => Ok(UserAction::Quit),
        _ => {
            println!("\nInvalid selection");
            Ok(UserAction::Cancelled)
        }
    }
}

/// Generates a commit message using the AI model
async fn generate_commit_message(
    model: &str,
    fixes: &Option<usize>,
    commit_history: &[String],
    diff: &str,
) -> Result<String> {
    let agent = gemini::Client::from_env();
    run_with_progress(|| async {
        let commiter = MsgCompletion::new(agent.completion_model(model))
            .with_issue(fixes)
            .with_history(commit_history.to_vec())
            .with_diff(diff);

        commiter
            .prompt(commiter.prompt.as_str())
            .await
            .map_err(|e| anyhow!(e))
    })
    .await
}

/// Creates a git commit with the generated message
fn create_git_commit(msg: &str) -> Result<String> {
    let repo = Repository::open(".")?;
    let sig = repo.signature()?;
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    let parents = match repo.head() {
        Ok(head) => vec![head.peel_to_commit()?],
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => vec![],
        Err(e) => return Err(e.into()),
    };

    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        msg,
        &tree,
        parents.iter().collect::<Vec<_>>().as_slice(),
    )?;

    println!("{}", "Created git commit successfully".bright_green());
    Ok(msg.to_string())
}

async fn handle_review_action(model: &str) -> Result<String> {
    let agent = gemini::Client::from_env();

    let diff = get_staged_diff().context("Getting staged changes failed")?;

    let msg = run_with_progress(|| async {
        let reviewer = Reviewer::new(agent.completion_model(model)).with_diff(&diff);

        reviewer
            .prompt(reviewer.prompt.as_str())
            .await
            .map_err(|e| anyhow!(e))
    })
    .await?;

    Ok(msg)
}

/// Get the last Nth commits from the repository
pub fn get_recent_commits(
    repo_path: &Path,
    history: &usize,
    ignore_patterns: Option<&[&str]>,
) -> Result<Vec<String>> {
    let repo = Repository::open(repo_path).context("Opening git repository failed")?;
    let head_commit = get_head_commit(&repo)?;
    let revwalk = setup_revision_walker(&repo, &head_commit)?;

    Ok(revwalk
        .take(*history)
        .filter_map(|id| process_commit(&repo, id, ignore_patterns))
        .collect())
}

fn get_head_commit(repo: &Repository) -> Result<git2::Commit> {
    repo.head()
        .context("Getting repository HEAD failed")?
        .peel_to_commit()
        .context("Getting HEAD commit failed")
}

fn setup_revision_walker<'a>(
    repo: &'a Repository,
    head_commit: &git2::Commit,
) -> Result<git2::Revwalk<'a>> {
    let mut revwalk = repo.revwalk().context("Creating revision walker failed")?;
    revwalk
        .push(head_commit.id())
        .context("Setting starting commit failed")?;
    revwalk
        .set_sorting(git2::Sort::TIME)
        .context("Setting sort order failed")?;

    Ok(revwalk)
}

fn process_commit(
    repo: &Repository,
    id: Result<git2::Oid, git2::Error>,
    ignore_patterns: Option<&[&str]>,
) -> Option<String> {
    id.ok().and_then(|id| {
        let commit = repo.find_commit(id).ok()?;
        if should_ignore_commit(&commit, ignore_patterns) {
            return None;
        }

        Some(format!(
            "{} - {}: {}",
            commit.id(),
            commit.author().name().unwrap_or("Unknown"),
            commit.message().unwrap_or("No message")
        ))
    })
}

fn should_ignore_commit(commit: &git2::Commit, ignore_patterns: Option<&[&str]>) -> bool {
    if let Some(patterns) = ignore_patterns {
        if let Ok(tree) = commit.tree() {
            for pattern in patterns {
                if tree.iter().any(|entry| {
                    entry
                        .name()
                        .map(|name| name.contains(pattern))
                        .unwrap_or(false)
                }) {
                    return true;
                }
            }
        }
    }
    false
}
