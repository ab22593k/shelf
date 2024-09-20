#![allow(clippy::nonminimal_bool)]

mod dotfile;
mod index;
pub mod suggestions;

use clap::{Parser, Subcommand};
pub use index::SlfIndex;
use std::path::PathBuf;
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help = true)]
pub struct SlfCLI {
    #[command(subcommand)]
    pub command: SlfActions,
}

#[derive(Subcommand)]
pub enum SlfActions {
    /// Track a new dotfile or multiple dotfiles for management
    /// This command adds the specified file(s) to the list of tracked dotfiles,
    /// allowing them to be synchronized across different environments.
    #[command(about = "Track a new dotfile or multiple dotfiles for management")]
    Track {
        /// Path to the dotfile(s) to be tracked
        /// This can be a single file or a directory containing multiple dotfiles.
        path: PathBuf,
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
}
