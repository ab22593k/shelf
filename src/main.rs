mod ai;
mod app;
mod bo;
mod configure;
mod spinner;
mod storage;

use crate::{configure::Config, storage::initialize_database};
use ai::utils::{handle_ai_commit, handle_ai_review};
use anyhow::{Context, Result};
use app::{AIAction, BoAction, Commands, Shelf};
use bo::{
    suggest::handle_fs_suggest,
    utils::{handle_bo_list, handle_bo_track, handle_bo_untrack},
};
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Generator};
use configure::handle_ai_config;
use std::{fs, io};

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) -> Result<()> {
    let bin_name = cmd.get_bin_name().unwrap_or("shelf").to_string();
    generate::<G, _>(gen, cmd, bin_name, &mut io::stdout());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Shelf::parse();
    let config = Config::default();

    // Ensure config directory exists
    if !config.path.exists() {
        fs::create_dir_all(&config.path).with_context(|| {
            format!(
                "Failed to create `shelf` config directory: {}",
                config.path.display()
            )
        })?;
    }

    // Database initialization and migration
    let conn = storage::establish_connection().await?;
    initialize_database(&conn).await?;

    match cli.command {
        Commands::Bo { action } => match action {
            BoAction::List { modified } => handle_bo_list(&conn, modified).await?,
            BoAction::Untrack { recursive, paths } => {
                handle_bo_untrack(&conn, recursive, paths).await?
            }
            BoAction::Track {
                paths,
                recursive,
                restore,
            } => handle_bo_track(&conn, recursive, restore, paths).await?,
            BoAction::Suggest { interactive } => handle_fs_suggest(&conn, interactive).await?,
        },
        Commands::Ai { action } => match action {
            AIAction::Commit {
                provider: provider_override,
                model: model_override,
            } => handle_ai_commit(config, provider_override, model_override).await?,
            AIAction::Review {
                provider: provider_override,
                model: model_override,
            } => handle_ai_review(config, provider_override, model_override).await?,
            AIAction::Config { action } => handle_ai_config(config, action).await?,
        },

        Commands::Completion { shell } => {
            let mut cmd = Shelf::command();
            print_completions(shell, &mut cmd)?;
        }
    }

    Ok(())
}
