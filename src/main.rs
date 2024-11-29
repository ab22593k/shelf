mod ai;
mod app;
mod config;
mod df;
mod spinner;

use crate::config::ShelfConfig;

use ai::{handle_ai_commit, handle_ai_review};
use anyhow::{Context, Result};
use app::{AIAction, Commands, DfAction, Shelf};
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Generator};
use colored::*;
use config::handle_ai_config;
use df::{
    handler::{handle_fs_list, handle_fs_track, handle_fs_untrack},
    suggest::handle_fs_suggest,
};
use rusqlite::Connection;

use std::{fs, io};

pub const DF_STORAGE_FILENAME: &str = "storage.sqlite";

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) -> Result<()> {
    let bin_name = cmd.get_bin_name().unwrap_or("shelf").to_string();
    generate::<G, _>(gen, cmd, bin_name, &mut io::stdout());
    Ok(())
}

async fn initialize_database(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS dotconf (
            path TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            last_modified INTEGER NOT NULL
        )",
        [],
    )
    .context("Failed to create table")?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Shelf::parse();
    let config = ShelfConfig::default();
    let storage_file = directories::BaseDirs::new()
        .map(|base| base.data_dir().join("shelf").join(DF_STORAGE_FILENAME))
        .expect("Could not create `shelf` data directory");

    if !config.path.exists() {
        fs::create_dir_all(&config.path).context("Failed to create `shelf` config directory")?;
    }

    let conn = Connection::open(&storage_file).context("Failed to open database")?;
    initialize_database(&conn).await?;

    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;",
    )
    .context("Failed to set pragmas")?;

    match cli.command {
        Commands::Df { actions } => match actions {
            DfAction::List { modified } => handle_fs_list(&conn, modified).await?,
            DfAction::Untrack { recursive, paths } => {
                handle_fs_untrack(&conn, recursive, paths).await?
            }
            DfAction::Track {
                recursive,
                restore,
                paths,
            } => handle_fs_track(&conn, recursive, restore, paths).await?,
            DfAction::Suggest { interactive } => handle_fs_suggest(&conn, interactive).await?,
        },
        Commands::Ai { actions } => match actions {
            AIAction::Commit {
                provider: provider_override,
                model: model_override,
                install_hook,
                remove_hook,
            } => {
                handle_ai_commit(
                    config,
                    provider_override,
                    model_override,
                    install_hook,
                    remove_hook,
                )
                .await?
            }
            AIAction::Review {
                provider: provider_override,
                model: model_override,
            } => handle_ai_review(config, provider_override, model_override).await?,
            AIAction::Config { action } => handle_ai_config(config, action).await?,
        },
        Commands::Migrate { fix } => {
            if fix {
                // Perform legacy config file migration
                let dotconf_file = config.path.join("dotconf.db");
                let gitai_file = config.path.join("gitai.json");
                let new_ai_file = gitai_file.parent().unwrap().join("ai.json");

                // Check if files exist and migration is needed
                let should_migrate = dotconf_file.exists() || gitai_file.exists();
                let can_migrate = !storage_file.exists() && gitai_file.exists();

                if should_migrate {
                    if can_migrate {
                        eprintln!(
                            "{}",
                            "Error: Database already exists at new location.".red()
                        );

                        return Ok(());
                    }

                    // Perform migration
                    if dotconf_file.exists() {
                        fs::rename(&dotconf_file, &storage_file)?;
                    }
                    if gitai_file.exists() {
                        fs::rename(&gitai_file, &new_ai_file)?;
                    }

                    println!("{}", "Migration successfully Done".green(),);
                }

                // Add more migrations here as needed, e.g.:
                // - Database schema changes
                // - Config file format changes
                // - File location changes
            } else {
                println!("Run with `--fix` to perform the following migrations:");
                println!(
                    "{}",
                    "   - Move Database to the appropriate location".bright_magenta()
                );
                println!("{}", "   - Rename gitai.json to ai.json".bright_magenta());
            }
        }
        Commands::Completion { shell } => print_completions(shell, &mut Shelf::command())?,
    }

    Ok(())
}
