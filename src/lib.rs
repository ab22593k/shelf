use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct SlfCLI {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Track and manage dotfiles across systems,
    #[command(
        short_flag = 'd',
        long_flag = "dotfiles",
        about = "Track and manage dotfiles across systems",
        long_about = "This command allows you to track, sync, and manage your dotfiles across multiple systems. Use subcommands to track new dotfiles, list existing ones, or perform other management tasks."
    )]
    Dotfiles {
        #[command(subcommand)]
        action: SlfActions,
    },
}

/// The `Commands` enum represents the main commands available in the CLI.
/// Currently, it only contains the `Dotfiles` command, but more commands can be added here in the future.
///
/// The `DotfilesAction` enum represents the various actions that can be performed on dotfiles.
/// These actions include tracking new dotfiles, listing existing ones, removing tracked dotfiles,
/// and syncing dotfiles to an output directory.
#[derive(Subcommand)]
pub enum SlfActions {
    /// Track a new dotfile or multiple dotfiles
    ///
    /// This action allows you to add one or more dotfiles to the tracking system.
    /// The path can be a single file or a directory containing multiple dotfiles. /// Track a new dotfile[s]...
    Track {
        /// Path to the dotfile[s]...
        path: PathBuf,
    },
    /// List tracked dotfiles
    ///
    /// This action displays a list of all dotfiles currently being tracked by the system.
    /// It provides an overview of the dotfiles under management, which can be useful for
    /// reviewing your tracked files or before performing sync operations.
    List,

    /// Remove a tracked dotfile from the system.
    ///
    /// This action allows you to stop tracking a specific dotfile. It removes the file
    /// from the tracking system but does not delete the actual file from your system.
    /// This can be useful if you no longer want to manage a particular dotfile or if
    /// you want to exclude it from future sync operations.emove a tracked dotfile.
    Remove {
        /// Path to the dotfile to remove
        path: PathBuf,
    },
    /// Sync tracked dotfiles to an output directory
    ///
    /// This action creates symlinks for all tracked dotfiles in the specified output directory.
    /// It allows you to easily replicate your dotfile configuration across different systems
    /// or create backups. The sync operation preserves the original file structure and
    /// ensures that any changes made to the original dotfiles are reflected in the synced versions.
    ///
    /// The `output_dir` parameter specifies where the symlinks will be created. If the directory
    /// doesn't exist, it will be created. Existing symlinks in the output directory will be
    /// updated to reflect any changes in the tracked dotfiles.nc tracked dotfiles
    Sync {
        /// Output directory for synced dotfiles
        #[arg(short, long)]
        outdir: PathBuf,
    },
}
