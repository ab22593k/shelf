#![allow(clippy::nonminimal_bool)]

mod dotfile;
pub mod index;

use clap::{Parser, Subcommand};
pub use index::SlfIndex;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct SlfCLI {
    #[command(subcommand)]
    pub command: Option<SlfActions>,
}

#[derive(Subcommand)]
pub enum SlfActions {
    /// Track a new dotfile or multiple dotfiles
    #[command(about = "Track a new dotfile or multiple dotfiles")]
    Track {
        /// Path to the dotfile[s]...
        path: PathBuf,
    },
    /// List tracked dotfiles
    #[command(about = "List tracked dotfiles")]
    List,
    /// Remove a tracked dotfile
    #[command(about = "Remove a tracked dotfile")]
    Remove {
        /// Path to the dotfile to remove
        path: PathBuf,
    },
    /// Sync tracked dotfiles
    #[command(about = "Sync tracked dotfiles")]
    Sync,
}
