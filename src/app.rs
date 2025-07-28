use anyhow::{Context, Result, anyhow};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use colored::Colorize;
use git2::Repository;
use rig::{
    client::{CompletionClient, ProviderClient},
    providers::gemini::{
        self,
        completion::gemini_api_types::{self, Part},
    },
};
use std::{
    borrow::Cow,
    collections,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::NamedTempFile;
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
        #[arg(short, long, default_value = "gemini-2.0-flash")]
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
        #[arg(short, long, default_value = "gemini-2.0-flash")]
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
            let reviews = handle_review_action(model.as_str()).await?;
            println!("{reviews}");
        }
        Commands::Completion { shell } => {
            let mut cmd = Shelf::command();
            let script =
                completions_script(*shell, &mut cmd).context("Printing completions failed")?;
            println!("{script}");
        }
    }
    Ok(())
}

async fn handle_dotfs_command(action: &FileAction, mut repo: DotFs) -> Result<()> {
    match action {
        FileAction::Track { paths } => tracking_handler(paths, &mut repo)?,
        FileAction::Untrack { paths } => untracking_handler(paths, &mut repo)?,
        FileAction::List { dirty } => display_files(repo, *dirty)?,
        FileAction::Save => saving_handler(&mut repo).await?,
    }
    Ok(())
}

async fn saving_handler(repo: &mut DotFs) -> Result<()> {
    // Save changes locally
    repo.save_local_changes()
        .context("Failed to save dotfs changes")?;

    print_success_message("DotFs saved successfully");

    Ok(())
}

fn print_path_status(path: &Path, action: &str, is_success: bool) {
    let colored_display = if is_success {
        path.display().to_string().bright_green()
    } else {
        path.display().to_string().bright_red()
    };
    println!("{action} {colored_display}");
}

fn tracking_handler(paths: &[PathBuf], repo: &mut DotFs) -> Result<()> {
    repo.track(paths).context("Tracking files failed")?;
    for path in paths {
        print_path_status(path, "Tracking", true);
    }
    Ok(())
}

fn untracking_handler(paths: &[PathBuf], repo: &mut DotFs) -> Result<()> {
    repo.untrack(paths).context("Untracking files failed")?;
    for path in paths {
        print_path_status(path, "Untracking", false);
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

    print_grouped_paths(&paths_by_dir);
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

fn get_home_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/")) // Fallback to root if home dir cannot be determined
}

fn display_path_relative_to_home<'a>(path: &'a Path, home: &'a Path) -> Cow<'a, Path> {
    path.strip_prefix(home)
        .map_or_else(|_| Cow::Borrowed(path), Cow::Borrowed)
}

fn print_directory_header(index: usize, total_dirs: usize, dir_path: &Path, home_dir: &Path) {
    let display_path = display_path_relative_to_home(dir_path, home_dir);
    let prefix = if index == total_dirs - 1 {
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

fn print_file_entry(index: usize, total_files: usize, file_path: &Path, home_dir: &Path) {
    let display_path = display_path_relative_to_home(file_path, home_dir);
    let file_type = if file_path.is_dir() { "📁" } else { "" };
    let prefix = if index == total_files - 1 {
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

fn print_grouped_paths(paths_by_dir: &collections::BTreeMap<PathBuf, Vec<PathBuf>>) {
    let home = get_home_dir();
    let total_dirs = paths_by_dir.len();

    for (i, (dir, files)) in paths_by_dir.iter().enumerate() {
        debug!("Processing directory: {:?} with {} files", dir, files.len());
        if !dir.as_os_str().is_empty() {
            print_directory_header(i, total_dirs, dir, &home);
        }

        let total_files = files.len();
        for (j, file) in files.iter().enumerate() {
            print_file_entry(j, total_files, file, &home);
        }
    }
}

async fn generate_commit_message(
    prefix: &str,
    model: &str,
    history: &usize,
    ignored: &Option<Vec<String>>,
) -> Result<String> {
    let response = commit_completion(prefix, model, history, ignored).await?;

    // If the response is a Gemini JSON structure, extract the commit message; otherwise, use the plain string
    let commit_msg = if let Ok(parsed) =
        serde_json::from_str::<gemini_api_types::GenerateContentResponse>(&response)
    {
        extract_commit_message(&parsed)
    } else {
        response
    };
    Ok(commit_msg)
}

async fn handle_commit_action(
    prefix: &str,
    model: &str,
    history: &usize,
    ignored: &Option<Vec<String>>,
) -> Result<String> {
    let mut current_commit_msg = String::new();

    loop {
        if current_commit_msg.is_empty() {
            // Generate commit message using AI model only if not already generated or edited
            current_commit_msg = generate_commit_message(prefix, model, history, ignored).await?;
        }

        println!("{current_commit_msg}",);

        // Get user action selection
        let selection = user_selection()?;
        match selection {
            UserAction::RegenerateMessage => {
                // Clear to force regeneration
                current_commit_msg.clear();

                continue;
            }
            UserAction::EditWithEditor => {
                current_commit_msg = edit_message_with_editor(&current_commit_msg)
                    .context("Failed to edit commit message with editor")?;

                // After editing, show the new message and prompt again
                continue;
            }
            UserAction::CommitChanges => return create_git_commit(current_commit_msg),
            UserAction::Quit => return Ok("Quitting".to_string()),
            UserAction::Cancelled => return Ok("Cancelled".to_string()),
        }
    }
}

fn edit_message_with_editor(initial_message: &str) -> Result<String> {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().context("Failed to create temporary file")?;

    // Write the initial message to the temporary file
    std::io::Write::write_all(&mut temp_file, initial_message.as_bytes())
        .context("Failed to write initial message to temporary file")?;

    // Determine the editor to use
    let editor = std::env::var("GIT_EDITOR")
        .or_else(|_| std::env::var("EDITOR"))
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "windows") {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        });

    // Spawn the editor process
    let status = Command::new(&editor)
        .arg(temp_file.path())
        .status()
        .with_context(|| format!("Failed to open editor: {editor}",))?;

    if !status.success() {
        return Err(anyhow!("Editor exited with a non-zero status."));
    }

    // Read the modified content from the temporary file
    let edited_message = std::fs::read_to_string(temp_file.path())
        .context("Failed to read edited message from temporary file")?;

    // The temporary file will be automatically deleted when `temp_file` goes out of scope

    Ok(edited_message)
}

fn extract_commit_message(response: &gemini_api_types::GenerateContentResponse) -> String {
    response
        .candidates
        .first()
        .and_then(|candidate| candidate.content.parts.iter().next())
        .and_then(|part| match part {
            Part::Text(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| String::from("No commit message generated"))
}

enum UserAction {
    RegenerateMessage,
    CommitChanges,
    EditWithEditor,
    Quit,
    Cancelled,
}

fn user_selection() -> Result<UserAction> {
    use dialoguer::{Select, theme::ColorfulTheme};
    let options = vec![
        "Regenerate message",
        "Edit with Editor",
        "Commit changes",
        "Quit",
    ];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do next?")
        .default(0)
        .items(&options)
        .interact();

    match selection {
        Ok(0) => Ok(UserAction::RegenerateMessage),
        Ok(1) => Ok(UserAction::EditWithEditor),
        Ok(2) => Ok(UserAction::CommitChanges),
        Ok(3) => Ok(UserAction::Quit),
        _ => {
            println!("\nInvalid selection");
            Ok(UserAction::Cancelled)
        }
    }
}
/// Creates a git commit with the generated message
fn create_git_commit(msg: String) -> Result<String> {
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
        &msg,
        &tree,
        parents.iter().collect::<Vec<_>>().as_slice(),
    )?;

    print_success_message("Created git commit successfully");
    Ok(msg)
}

async fn handle_review_action(model: &str) -> Result<String> {
    let agent = gemini::Client::from_env();

    let diff = get_staged_diff().context("Getting staged changes failed")?;

    let msg = run_with_progress(|| async {
        let reviewer = Reviewer::new(agent.completion_model(model)).with_diff(&diff);

        reviewer.review().await.map_err(|e| anyhow!(e))
    })
    .await?;

    Ok(msg)
}

fn print_success_message(msg: &str) {
    println!("{}", msg.bright_green());
}
