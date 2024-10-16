#![allow(clippy::nonminimal_bool)]

mod dotfile;
mod suggestions;

use anyhow::{anyhow, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use dotfile::Dotfiles;
use suggestions::Suggestions;

use std::{io, path::PathBuf};

#[derive(Parser)]
#[command(author, about, long_about = None ,version)]
#[command(arg_required_else_help = true)]
pub struct Shelf {
    #[command(subcommand)]
    pub command: Actions,
}

#[derive(Subcommand)]
pub enum Actions {
    /// Track one or more new dotfiles for management
    /// This command adds the specified file(s) to the list of tracked dotfiles,
    /// allowing them to be synchronized across different environments.
    /// Multiple files or directories can be specified at once.
    #[command(about = "Track one or more new dotfiles for management")]
    Add {
        /// Paths to the dotfiles to be tracked
        /// These can be individual files or directories containing dotfiles.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },

    /// List all currently tracked dotfiles
    /// This command displays a comprehensive list of all dotfiles that are
    /// currently being managed by the system, including their paths and status.
    #[command(name = "ls", about = "List all currently tracked dotfiles")]
    List,

    /// Remove a tracked dotfile from management
    /// This command stops tracking the specified dotfile, removing it from
    /// the list of managed files without deleting the actual file.
    #[command(name = "rm", about = "Remove a tracked dotfile from management")]
    Remove {
        /// Path to the dotfile to be removed from tracking
        /// This should be the path of a currently tracked dotfile.
        path: PathBuf,
    },

    /// Create symlinks for all tracked dotfiles
    /// This command creates or updates symlinks in the target directory for all
    /// tracked dotfiles, ensuring they are linked to their correct locations.
    #[command(name = "cp", about = "Create symlinks for all tracked dotfiles")]
    Copy,

    /// Suggest commonly used configuration files
    /// This command provides a list of popular dotfiles and configuration
    /// files commonly used across Linux and macOS systems.
    #[command(about = "Suggest commonly used configuration files")]
    Suggest {
        /// Enable interactive mode for selecting dotfiles
        #[arg(short, long)]
        interactive: bool,
    },

    /// Generate shell completion scripts.
    #[command(about = "Generate shell completion scripts")]
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

fn setup_config_dir() -> Result<PathBuf> {
    let d = directories::BaseDirs::new()
        .map(|base_dirs| base_dirs.config_dir().join("shelf"))
        .or_else(|| {
            std::env::var("XDG_CONFIG_HOME")
                .ok()
                .map(|x| PathBuf::from(x).join("shelf"))
        })
        .or_else(|| home::home_dir().map(|x| x.join(".config").join("shelf")))
        .unwrap_or_else(|| {
            eprintln!("Warning: Could not determine config directory. Using current directory.");
            std::env::current_dir().unwrap().join(".shelf")
        });

    Ok(d)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Shelf::parse();
    let config_dir = setup_config_dir()?;
    let target_directory = config_dir.join("dotfiles");
    let index_file_path = config_dir.join("index.json");

    tokio::fs::create_dir_all(&target_directory).await?;

    let mut df = Dotfiles::load(config_dir.clone(), target_directory).await?;

    match cli.command {
        Actions::Add { paths } => {
            df.add_multi(paths).await;
        }
        Actions::List => df.print_list(),
        Actions::Remove { path } => {
            let results = df.remove_multi(&[path.to_str().unwrap()]);
            if results.iter().any(|r| r.is_err()) {
                return Err(anyhow!("Failed to remove one or more dotfiles"));
            }
        }
        Actions::Copy => {
            df.copy().await?;
        }
        Actions::Suggest { interactive } => {
            Suggestions::default().render(&mut df, interactive).await?
        }
        Actions::Completion { shell } => {
            let mut cmd = Shelf::command();
            print_completions(shell, &mut cmd);
            return Ok(());
        }
    }

    df.save(&index_file_path).await?;
    Ok(())
}
