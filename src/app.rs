use clap::{Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

/// A CLI tool for managing system configuration files, providing AI assistance, and automating migration.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)] // Ensure version propagates to subcommands
pub struct Shelf {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage system configuration files.
    Bo {
        #[command(subcommand)]
        action: BoAction,
    },

    /// AI assistance for code review and commit message generation.
    Ai {
        #[command(subcommand)]
        action: AIAction,
    },

    /// Generate shell completion scripts.
    Completion {
        /// The shell to generate completions for.
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand)]
pub enum BoAction {
    /// Track files for management.
    Track {
        /// Paths to the files to track.
        paths: Vec<PathBuf>,
        /// Perform the operation recursively for directories.
        #[arg(short, long)]
        recursive: bool,
        /// Restore file content from the database.
        #[arg(long)]
        restore: bool,
    },
    /// Remove files from management.
    Untrack {
        /// Paths to the files to untrack.
        paths: Vec<PathBuf>,
        /// Perform the operation recursively for directories.
        #[arg(short, long)]
        recursive: bool,
    },
    /// List all currently tracked files.
    List {
        /// List only modified files.
        #[arg(short, long)]
        modified: bool,
    },
    /// Suggest commonly used configuration files.
    Suggest {
        /// Run in interactive mode.
        #[arg(short, long)]
        interactive: bool,
    },
}

#[derive(Subcommand)]
pub enum AIAction {
    /// Generate a commit message using AI or manage git hooks.
    Commit {
        /// Override the configured AI provider.
        #[arg(short, long, value_parser = ["groq", "xai", "gemini", "anthropic", "openai", "ollama"])]
        provider: Option<String>,
        /// Override the configured model.
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Review code changes and suggest improvements using AI.
    Review {
        /// Override the configured AI provider.
        #[arg(short, long)]
        provider: Option<String>,
        /// Override the configured model.
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Configure AI settings.
    Config {
        #[command(subcommand)]
        action: AIConfigAction,
    },
}

#[derive(Subcommand)]
pub enum AIConfigAction {
    /// Set a configuration value.
    Set {
        /// Configuration key.
        #[arg(value_enum, value_parser = ["provider", "model", "groq_api_key", "xai_api_key", "gemini_api_key", "anthropic_api_key", "openai_api_key"])]
        key: String,
        /// Configuration value.
        value: String,
    },
    /// Get a configuration value.
    Get {
        /// Configuration key.
        key: String,
    },
    /// List all configuration values.
    List,
    /// Manage the commit message git hook.
    Hook {
        /// Install the prepare-commit-msg hook.
        #[arg(short, long)]
        install: bool,
        /// Uninstall the prepare-commit-msg hook.
        #[arg(short, long)]
        uninstall: bool,
    },
}
