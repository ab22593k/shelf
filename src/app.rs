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
use tracing::debug;

use crate::{
    commit::commit_completion,
    dotfs::{DotFs, ListFilter},
    shell::completions_script,
};
use crate::{
    review::Reviewer,
    utils::{get_staged_diff, run_with_progress},
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
        /// Suitable continuation context for the commit message.
        #[arg(short, long)]
        prefix: String,
        /// Override the configured model.
        #[arg(short, long, default_value = "gemini-2.0-flash-lite")]
        model: String,
        /// Include the nth commit history.
        #[arg(long, short = 'd', default_value = "10")]
        history_depth: usize,
        /// Ignore specific files or patterns when generating a commit (comma-separated).
        #[arg(short, long, default_value = None, value_delimiter = ',', num_args = 1..)]
        ignored: Option<Vec<String>>,
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
    /// Save files for management.
    Save,
}

pub async fn run_app(cli: Shelf, repo: DotFs) -> Result<()> {
    match &cli.command {
        Commands::Dotfs { action } => {
            handle_dotfs_command(action, repo).await?;
        }
        Commands::Commit {
            prefix,
            model,
            history_depth,
            ignored,
        } => {
            handle_commit_action(prefix, model.as_str(), history_depth, ignored).await?;
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

async fn handle_dotfs_command(action: &FileAction, mut repo: DotFs) -> Result<()> {
    match action {
        FileAction::Track { paths } => tracking_handler(paths, &mut repo)?,
        FileAction::Untrack { paths } => untracking_handler(paths, &mut repo)?,
        FileAction::List { dirty } => display_files(repo, *dirty)?,
        FileAction::Save {} => saving_handler(&mut repo).await?,
    }
    Ok(())
}

async fn saving_handler(repo: &mut DotFs) -> Result<()> {
    // Save changes locally
    if let Err(e) = repo.save_local_changes() {
        return Err(anyhow!(format!("Failed to save dotfs changes: {}", e)));
    }

    println!("{}", "DotFs saved successfully".bright_green());

    Ok(())
}

fn tracking_handler(paths: &[PathBuf], repo: &mut DotFs) -> Result<()> {
    repo.track(paths).context("Tracking files failed")?;
    for path in paths {
        println!("Tracking {}", path.display().to_string().bright_green());
    }
    Ok(())
}

fn untracking_handler(paths: &[PathBuf], repo: &mut DotFs) -> Result<()> {
    repo.untrack(paths).context("Untracking files failed")?;
    for path in paths {
        println!("Untracking {}", path.display().to_string().bright_red());
    }
    Ok(())
}

fn display_files(mut repo: DotFs, dirty: bool) -> Result<()> {
    debug!("Displaying files with dirty: {}", dirty);
    let paths = match dirty {
        true => {
            repo.set_filter(ListFilter::Modified);
            repo.collect()
        }
        false => repo.collect(),
    };
    let paths_by_dir = group_tabs_by_directory(paths);

    // Get the home directory for displaying relative paths
    let user_dirs = directories::UserDirs::new().unwrap();
    let home = user_dirs.home_dir();
    for (i, (dir, files)) in paths_by_dir.clone().into_iter().enumerate() {
        debug!("Processing directory: {:?} with {} files", dir, files.len());
        if !dir.as_os_str().is_empty() {
            let display_path = dir.strip_prefix(home).unwrap_or(&dir);
            let prefix = if i == paths_by_dir.len() - 1 {
                "└── "
            } else {
                "├── "
            };
            println!(
                "{}{}{}{}",
                prefix.blue().bold(),
                "📁 ".blue().bold(),
                display_path.display().to_string().blue().bold(),
                ":".blue().bold()
            );
        }

        for (j, file) in files.clone().into_iter().enumerate() {
            let display_path = file.strip_prefix(home).unwrap_or(&file);
            let file_type = if file.is_dir() { "📁" } else { "" };
            let prefix = if j == files.len() - 1 {
                "    └── "
            } else {
                "    ├── "
            };
            println!(
                "{}{} {}",
                prefix.bright_green(),
                file_type.bright_green(),
                display_path.display()
            );
        }
    }
    Ok(())
}

fn group_tabs_by_directory(paths: Vec<PathBuf>) -> collections::BTreeMap<PathBuf, Vec<PathBuf>> {
    debug!("Grouping {} paths by directory", paths.len());
    let mut paths_by_dir: collections::BTreeMap<PathBuf, Vec<PathBuf>> =
        collections::BTreeMap::new();
    for file in paths {
        let parent = file.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
        paths_by_dir.entry(parent).or_default().push(file);
    }

    paths_by_dir
}

async fn handle_commit_action(
    prefix: &str,
    model: &str,
    history: &usize,
    ignored: &Option<Vec<String>>,
) -> Result<String> {
    loop {
        let raw_response = commit_completion(prefix, model, history, ignored)
            .await?
            .to_string();
        debug!("{}", raw_response);

        // Get user action selection
        let selection = user_selection()?;
        match selection {
            UserAction::RegenerateMessage => continue,
            UserAction::CommitChanges => return create_git_commit(&raw_response),
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
