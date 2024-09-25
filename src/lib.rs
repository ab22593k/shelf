use std::path::PathBuf;

use clap::{arg, command, Parser, Subcommand};
use clap_complete::Shell;

pub mod dotfile;
pub mod github;
pub mod suggestions;

#[derive(Parser)]
#[command(author, about, long_about = None)]
#[command(arg_required_else_help = true)]
#[command(disable_version_flag = true)]
pub struct Shelf {
    /// Print version information
    #[arg(short = 'v', long = "version", action = clap::ArgAction::Version)]
    version: Option<bool>,

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

    /// Repository operations for syncing dotfiles
    #[command(about = "Repository operations for syncing dotfiles")]
    Repo {
        #[arg(required = true)]
        path: PathBuf,

        #[arg(long, conflicts_with = "pull")]
        push: bool,

        #[arg(long, conflicts_with = "push")]
        pull: bool,
    },

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
