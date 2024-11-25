mod ai;
mod app;
mod config;
mod fs;
mod spinner;

use anyhow::{Context, Result};
use app::{AIAction, AIConfigAction, Commands, FsAction, Shelf};
use chrono::{DateTime, Utc};
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Generator};
use colored::*;
use fs::{suggest::Suggestions, Fs};
use rusqlite::Connection;
use walkdir::WalkDir;

use std::{io, path::PathBuf, time::UNIX_EPOCH};

use crate::{
    ai::{
        prompt::PromptKind,
        providers::create_provider,
        utils::{get_diff_cached, install_git_hook, remove_git_hook},
        AIConfig,
    },
    config::ConfigOp,
};

// Helper function to format timestamps
fn format_timestamp(timestamp: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from(UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64))
}

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) -> Result<()> {
    let bin_name = cmd.get_bin_name().unwrap_or("shelf").to_string();
    generate::<G, _>(gen, cmd, bin_name, &mut io::stdout());
    Ok(())
}

async fn init_db(conn: &Connection) -> Result<()> {
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

async fn handle_fs_list(conn: &Connection, modified: bool) -> Result<()> {
    let mut stmt = conn.prepare("SELECT path, content, last_modified FROM dotconf")?;
    let files = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if files.is_empty() {
        println!("{}", "No tracked file found.".yellow());
        return Ok(());
    }

    println!("{}", "Tracked files:".green().bold());

    for (path, content, timestamp) in files {
        println!("*{}", "-".repeat(5).bright_black());
        let path_buf = PathBuf::from(&path);

        if modified {
            // Skip if file hasn't been modified
            if let Ok(metadata) = std::fs::metadata(&path_buf) {
                if let Ok(modified) = metadata.modified() {
                    let file_timestamp = modified
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    if file_timestamp <= timestamp {
                        continue;
                    }
                }
            }
        }

        let inserted = format_timestamp(timestamp);
        println!("{}: {}", "Path".blue().bold(), path);
        println!(
            "{}: {}",
            "Inserted".cyan().bold(),
            inserted.format("%Y-%m-%d %H:%M:%S UTC")
        );
        println!("{}: {} bytes", "Size".magenta().bold(), content.len());
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Shelf::parse();
    let conf_path = ConfigOp::create_dotconf_db().get_path();

    if let Some(parent) = conf_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create config directory")?;
    }

    let conn = Connection::open(&conf_path).context("Failed to open database")?;

    init_db(&conn).await?;

    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;",
    )
    .context("Failed to set pragmas")?;

    match cli.command {
        Commands::FS { actions } => match actions {
            FsAction::List { modified } => {
                handle_fs_list(&conn, modified).await?;
            }

            FsAction::Untrack { recursive, paths } => {
                for base_path in paths {
                    if recursive && base_path.is_dir() {
                        for entry in WalkDir::new(&base_path)
                            .follow_links(true)
                            .into_iter()
                            .filter_map(Result::ok)
                            .filter(|e| e.path().is_file())
                        {
                            if let Ok(dotconf) = Fs::select(&conn, entry.path()).await {
                                Fs::remove(&conn, entry.path()).await?;
                                println!(
                                    "{} {}",
                                    "Removed:".green().bold(),
                                    dotconf.get_path().display()
                                );
                            }
                        }
                    } else if let Ok(dotconf) = Fs::select(&conn, &base_path).await {
                        Fs::remove(&conn, &base_path).await?;
                        println!(
                            "{} {}",
                            "Removed:".green().bold(),
                            dotconf.get_path().display()
                        );
                    } else {
                        eprintln!("{} No such file: {:?}", "Error:".red().bold(), base_path);
                    }
                }
            }

            FsAction::Track {
                recursive,
                restore,
                paths,
            } => {
                for base_path in paths {
                    if recursive && base_path.is_dir() {
                        for entry in WalkDir::new(&base_path)
                            .follow_links(true)
                            .into_iter()
                            .filter_map(Result::ok)
                            .filter(|e| e.path().is_file())
                        {
                            let path = entry.path();
                            if restore {
                                if let Ok(mut dotconf) = Fs::select(&conn, path).await {
                                    dotconf.restore(&conn).await?;
                                    println!("{} {:?}", "Restored:".green().bold(), path);
                                }
                            } else if let Ok(mut dotconf) = Fs::from_file(path).await {
                                dotconf.insert(&conn).await?;
                                println!("{} {:?}", "Tracked:".green().bold(), path);
                            }
                        }
                    } else if restore {
                        if let Ok(mut dotconf) = Fs::select(&conn, &base_path).await {
                            dotconf.restore(&conn).await?;
                            println!("{} {:?}", "Restored:".green().bold(), base_path);
                        }
                    } else if let Ok(mut dotconf) = Fs::from_file(&base_path).await {
                        dotconf.insert(&conn).await?;
                        println!("{} {:?}", "Tracked:".green().bold(), base_path);
                    }
                }
            }

            FsAction::Suggest { interactive } => {
                let suggestions = Suggestions::default();

                if interactive {
                    match suggestions.interactive_selection() {
                        Ok(selected) => {
                            for path in selected {
                                let expanded_path = shellexpand::tilde(&path).to_string();
                                if let Ok(mut file) = Fs::from_file(expanded_path).await {
                                    if file.insert(&conn).await.is_ok() {
                                        println!("{} {}", "Added:".green().bold(), path);
                                    }
                                }
                            }
                        }
                        Err(e) => eprintln!("{} {}", "Selection failed:".red().bold(), e),
                    }
                } else {
                    suggestions.print_suggestions();
                }
            }
        },

        Commands::AI { actions } => match actions {
            AIAction::Commit {
                provider: provider_override,
                hooki,
                hookr,
            } => {
                let repo = git2::Repository::open_from_env()?;
                let hooks_dir = repo.path().join("hooks");

                if hooki {
                    return install_git_hook(&hooks_dir);
                }

                if hookr {
                    return remove_git_hook(&hooks_dir);
                }

                let mut config = AIConfig::load().await?;
                if let Some(provider_name) = provider_override {
                    config.provider = provider_name;
                }

                let provider = create_provider(&config)?;
                let commit_msg = spinner::new(|| async {
                    let diff = get_diff_cached()?;
                    provider
                        .generate_assistant_message(PromptKind::Commit, &diff)
                        .await
                })
                .await?;

                println!(
                    "{}\n{}",
                    "Generated commit message:".green().bold(),
                    commit_msg
                );
            }

            AIAction::Review {
                provider: provider_override,
            } => {
                let mut config = AIConfig::load().await?;
                if let Some(provider_name) = provider_override {
                    config.provider = provider_name;
                }

                let provider = create_provider(&config)?;
                let review = spinner::new(|| async {
                    let diff = get_diff_cached()?;
                    provider
                        .generate_assistant_message(PromptKind::Review, &diff)
                        .await
                })
                .await?;

                println!("{}\n{}", "Code review:".green().bold(), review);
            }

            AIAction::Config { action } => match action {
                AIConfigAction::Set { key, value } => {
                    let mut config = AIConfig::load().await?;
                    config.set(&key, &value)?;
                    config.save().await?;
                    println!("{} {} = {}", "Set:".green().bold(), key, value);
                }

                AIConfigAction::Get { key } => {
                    let config = AIConfig::load().await?;
                    if let Some(value) = config.get(&key) {
                        println!("{}", value);
                    } else {
                        println!("{} Key not found: {}", "Error:".red().bold(), key);
                    }
                }

                AIConfigAction::List => {
                    let config = AIConfig::load().await?;
                    println!("{}", "Configuration:".green().bold());
                    config
                        .list()
                        .iter()
                        .for_each(|(k, v)| println!("{} = {}", k, v));
                }
            },
        },

        Commands::Completion { shell } => print_completions(shell, &mut Shelf::command())?,
    }
    Ok(())
}
