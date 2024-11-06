use std::path::PathBuf;

use clap::{Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[command(author, about, long_about = None ,version)]
#[command(arg_required_else_help = true)]
pub struct Shelf {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Manage dotconf files")]
    Dotconf {
        #[command(subcommand)]
        actions: DotconfActions,
    },

    #[command(about = "Git commands with AI assistance")]
    Gitai {
        #[command(subcommand)]
        actions: GitAIActions,
    },

    #[command(about = "Generate shell completion scripts")]
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand)]
pub enum DotconfActions {
    #[command(name = "ls", about = "List all currently tracked dotconf files")]
    List,

    #[command(name = "rm", about = "Remove dotconf files from management")]
    Remove {
        #[arg(short, long)]
        recursive: bool,
        paths: Vec<PathBuf>,
    },

    #[command(name = "cp", about = "Create a dotconf files copy")]
    Copy {
        #[arg(short, long)]
        recursive: bool,
        #[arg(short, long)]
        backload: bool,
        paths: Vec<PathBuf>,
    },

    #[command(about = "Suggest commonly used dotconf[s] cross diffrent OS's")]
    Suggest {
        #[arg(short, long)]
        interactive: bool,
    },
}

#[derive(Subcommand)]
pub enum GitAIActions {
    #[command(
        name = "commit",
        about = "Generate a commit message using AI or install git hook"
    )]
    Commit {
        #[arg(short, long, help = "Override the configured AI provider")]
        provider: Option<String>,
        #[arg(long, help = "Install the prepare-commit-msg hook")]
        install: bool,
        #[arg(long, help = "Remove the prepare-commit-msg hook")]
        uninstall: bool,
    },

    #[command(name = "config", about = "Configure GitAI settings")]
    Config {
        #[command(subcommand)]
        action: GitAIConfigActions,
    },
}

#[derive(Subcommand)]
pub enum GitAIConfigActions {
    #[command(name = "set", about = "Set a configuration value")]
    Set {
        #[arg(help = "Configuration key")]
        key: String,
        #[arg(help = "Configuration value")]
        value: String,
    },

    #[command(name = "get", about = "Get a configuration value")]
    Get {
        #[arg(help = "Configuration key")]
        key: String,
    },

    #[command(name = "list", about = "List all configuration values")]
    List,
}
