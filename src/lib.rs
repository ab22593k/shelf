#![allow(clippy::nonminimal_bool)]

pub mod dotfile;
pub mod suggestions;

use clap_complete::Shell;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help = true)]
pub struct Slf {
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
    Track {
        /// Paths to the dotfiles to be tracked
        /// These can be individual files or directories containing dotfiles.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },

    /// List all currently tracked dotfiles
    /// This command displays a comprehensive list of all dotfiles that are
    /// currently being managed by the system, including their paths and status.
    #[command(about = "List all currently tracked dotfiles")]
    List,

    /// Remove a tracked dotfile from management
    /// This command stops tracking the specified dotfile, removing it from
    /// the list of managed files without deleting the actual file.
    #[command(about = "Remove a tracked dotfile from management")]
    Remove {
        /// Path to the dotfile to be removed from tracking
        /// This should be the path of a currently tracked dotfile.
        path: PathBuf,
    },

    /// Synchronize all tracked dotfiles across environments
    /// This command ensures that all tracked dotfiles are up-to-date and
    /// consistent across different systems or backup locations.
    #[command(about = "Synchronize all tracked dotfiles across environments")]
    Sync,
    /// Suggest commonly used configuration files
    /// This command provides a list of popular dotfiles and configuration
    /// files commonly used across Linux and macOS systems.
    #[command(about = "Suggest commonly used configuration files")]
    Suggest {
        /// Enable interactive mode for selecting dotfiles
        #[arg(short, long)]
        interactive: bool,
    },

    /// Generate shell completion scripts
    #[command(about = "Generate shell completion scripts")]
    Completion {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}
