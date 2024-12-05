use clap::{Parser, Subcommand};
use clap_complete::Shell;

use std::path::PathBuf;

#[derive(Parser)]
#[command(author, about, long_about = None ,version)]
#[command(arg_required_else_help = true)]
pub struct Shelf {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Manage system configuration files")]
    Bo {
        #[command(subcommand)]
        actions: BoAction,
    },

    #[command(about = "AI assistance")]
    Ai {
        #[command(subcommand)]
        actions: AIAction,
    },

    #[command(about = "Automatically fix breakings changes")]
    Migrate {
        #[arg(
            short,
            long,
            help = "Apply migration, if set to false, it will only show what would happen"
        )]
        fix: bool,
    },

    #[command(about = "Generate shell completion scripts")]
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand)]
pub enum BoAction {
    #[command(about = "Create a system file copy")]
    Track {
        paths: Vec<PathBuf>,

        // #[arg(short, long, help = "Encrypt files content")]
        // encrypt: bool,
        #[arg(short, long, help = "Perform the operation recursively")]
        recursive: bool,

        #[arg(long, help = "Copy files content to the source location")]
        restore: bool,
    },

    #[command(about = "Remove files from management")]
    Untrack {
        paths: Vec<PathBuf>,

        #[arg(short, long)]
        recursive: bool,
    },

    #[command(about = "List all currently tracked files")]
    List {
        #[arg(short, long, help = "List all modified files only")]
        modified: bool,
    },

    #[command(about = "Commonly used configuration files cross diffrent OS's")]
    Suggest {
        #[arg(short, long)]
        interactive: bool,
    },
}

#[derive(Subcommand)]
pub enum AIAction {
    #[command(
        about = "Generate a commit message using AI or install git hook",
        long_about = "Generates semantic commit messages by analyzing staged changes using AI.
Can also install/remove a git hook for automated message generation"
    )]
    Commit {
        #[arg(
            short,
            long, help = "Override the configured AI provider",
            value_enum,
            value_parser = ["groq", "xai", "gemini", "anthropic", "openai", "ollama"])]
        provider: Option<String>,

        #[arg(short, long, help = "Override the configured model")]
        model: Option<String>,
    },

    #[command(
        about = "Review code changes and suggest improvements using AI",
        long_about = "Analyzes staged changes, diffs, or specified files and provides suggestions
for code improvements, potential bugs, and best practices using AI"
    )]
    Review {
        #[arg(short, long, help = "Override the configured AI provider")]
        provider: Option<String>,

        #[arg(short, long, help = "Override the configured model")]
        model: Option<String>,
    },

    #[command(
        about = "Configure AI settings",
        long_about = "Available keys:
• provider: groq, xai, gemini, anthropic or openai
• openai_api_key
• anthropic_api_key
• gemini_api_key
• groq_api_key
• xai_api_key"
    )]
    Config {
        #[command(subcommand)]
        action: AIConfigAction,
    },
}

#[derive(Subcommand)]
pub enum AIConfigAction {
    #[command(about = "Set a configuration value")]
    Set {
        #[arg(
            help = "Configuration key",
            value_enum,
            value_parser = ["provider", "model", "groq_api_key", "xai_api_key", "gemini_api_key", "anthropic_api_key", "openai_api_key"]
        )]
        key: String,
        #[arg(help = "Configuration value")]
        value: String,
    },

    #[command(about = "Get a configuration value")]
    Get {
        #[arg(help = "Configuration key")]
        key: String,
    },

    #[command(about = "List all configuration values")]
    List,

    #[command(about = "Commit message git hook management")]
    Hook {
        #[arg(short, long, help = "Install the prepare-commit-msg hook")]
        install: bool,

        #[arg(short, long, help = "Uninstall the prepare-commit-msg hook")]
        uninstall: bool,
    },
}
