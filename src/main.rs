mod app;
mod config;
mod dotconf;
mod gitai;
mod spinner;

use crate::gitai::GitAIConfig;

use anyhow::Result;
use app::{Commands, DotconfActions, GitAIActions, GitAIConfigActions, Shelf};
use chrono::{DateTime, Utc};
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Generator};
use colored::*;
use config::ShelfConfig;
use dotconf::suggest::Suggestions;
use dotconf::Dotconf;
use gitai::git::{git_diff, install_git_hook, remove_git_hook};
use gitai::providers::create_provider;
use rusqlite::Connection;
use walkdir::WalkDir;

use std::{io, time::SystemTime};

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) -> Result<()> {
    let bin_name = cmd.get_bin_name().unwrap_or("slf").to_string();
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
    )?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Shelf::parse();
    let conf_path = ShelfConfig::dotconf_conf().get_path();
    if let Some(parent) = conf_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Ensure connection is properly initialized
    let conn = Connection::open(conf_path.clone())?;
    init_db(&conn).await?;

    // Create prepared statements for common operations
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
                   PRAGMA journal_mode = WAL;",
    )?;

    match cli.command {
        Commands::Dotconf { actions } => {
            match actions {
                DotconfActions::List { modified } => {
                    let conn = rusqlite::Connection::open(conf_path)?;

                    // Query all tr acked files
                    let mut stmt =
                        conn.prepare("SELECT path, content, last_modified FROM dotconf")?;
                    let files: Vec<_> = stmt
                        .query_map([], |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, i64>(2)?,
                            ))
                        })?
                        .collect::<Result<_, _>>()?;

                    if files.is_empty() {
                        println!("{}", "No tracked dotconf[s] found.".yellow());
                        return Ok(());
                    }
                    println!("{}", "Tracked dotconf[s]:".green().bold());
                    println!("{}", "=================".bright_black());

                    for file in files {
                        let (path, content, timestamp) = file;

                        // Check if file exists and get its last modified time
                        let path_buf = std::path::PathBuf::from(&path);

                        if modified {
                            if let Ok(metadata) = std::fs::metadata(&path_buf) {
                                if let Ok(modified) = metadata.modified() {
                                    let file_timestamp = modified
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs()
                                        as i64;

                                    // Skip if file hasn't been modified since last tracking
                                    if file_timestamp <= timestamp {
                                        continue;
                                    }
                                }
                            }
                        }

                        let inserted = DateTime::<Utc>::from(
                            SystemTime::UNIX_EPOCH
                                + std::time::Duration::from_secs(timestamp as u64),
                        );
                        println!("{}: {}", "Path".blue().bold(), path);
                        println!(
                            "{}: {}",
                            "Inserted".cyan().bold(),
                            inserted.format("%Y-%m-%d %H:%M:%S UTC")
                        );
                        println!("{}: {} bytes", "Size".magenta().bold(), content.len());
                        println!("{}", "=================".bright_black());
                    }
                }

                DotconfActions::Remove { recursive, paths } => {
                    let conn = Connection::open(&conf_path)?;
                    for base_path in paths {
                        if recursive && base_path.is_dir() {
                            for entry in WalkDir::new(&base_path)
                                .follow_links(true)
                                .into_iter()
                                .filter_map(|e| e.ok())
                            {
                                let path = entry.path();
                                if path.is_file() {
                                    if let Ok(dotconf) = Dotconf::select(&conn, path).await {
                                        Dotconf::remove(&conn, path).await?;
                                        println!(
                                            "{} {}",
                                            "Removed:".green().bold(),
                                            dotconf.get_path().display()
                                        );
                                    }
                                }
                            }
                        } else if let Ok(dotconf) = Dotconf::select(&conn, &base_path).await {
                            Dotconf::remove(&conn, &base_path).await?;
                            println!(
                                "{} {}",
                                "Removed:".green().bold(),
                                dotconf.get_path().display()
                            );
                        } else {
                            eprintln!(
                                "{} No such dotconf found: {:?}",
                                "Error:".red().bold(),
                                base_path
                            );
                        }
                    }
                }
                DotconfActions::Copy {
                    recursive,
                    restore,
                    paths,
                } => {
                    let conn = Connection::open(&conf_path)?;

                    for base_path in paths {
                        if recursive && base_path.is_dir() {
                            for entry in WalkDir::new(&base_path)
                                .follow_links(true)
                                .into_iter()
                                .filter_map(|e| e.ok())
                            {
                                let path = entry.path();
                                if path.is_file() {
                                    if restore {
                                        if let Ok(mut dotconf) = Dotconf::select(&conn, path).await
                                        {
                                            dotconf.restore(&conn).await?;
                                            println!(
                                                "{} {:?}",
                                                "Successfully restored".green().bold(),
                                                path
                                            );
                                        }
                                    } else {
                                        match Dotconf::from_file(path).await {
                                            Ok(mut dotconf) => {
                                                dotconf.insert(&conn).await?;
                                                println!(
                                                    "{} {:?}",
                                                    "Successfully copied".green().bold(),
                                                    path
                                                );
                                            }
                                            Err(e) => {
                                                println!(
                                                    "{} {:?}: {}",
                                                    "Failed to copy".red().bold(),
                                                    path,
                                                    e
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        } else if restore {
                            if let Ok(mut dotconf) = Dotconf::select(&conn, &base_path).await {
                                dotconf.restore(&conn).await?;
                                println!(
                                    "{} {:?}",
                                    "Successfully restored".green().bold(),
                                    base_path
                                );
                            }
                        } else {
                            match Dotconf::from_file(&base_path).await {
                                Ok(mut dotconf) => {
                                    dotconf.insert(&conn).await?;
                                    println!(
                                        "{} {:?}",
                                        "Successfully copied".green().bold(),
                                        base_path
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "{} {:?}: {}",
                                        "Failed to copy".red().bold(),
                                        base_path,
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
                DotconfActions::Suggest { interactive } => {
                    let conn = Connection::open(&conf_path)?;
                    let suggestions = Suggestions::default();

                    if interactive {
                        match suggestions.interactive_selection() {
                            Ok(selected) => {
                                for path in selected {
                                    let expanded_path = shellexpand::tilde(&path).to_string();
                                    match Dotconf::from_file(expanded_path).await {
                                        Ok(mut dotconf) => match dotconf.insert(&conn).await {
                                            Ok(_) => {
                                                println!("{} {}", "Added".green().bold(), path)
                                            }
                                            Err(e) => eprintln!(
                                                "{} {}: {}",
                                                "Failed".red().bold(),
                                                path,
                                                e
                                            ),
                                        },
                                        Err(e) => {
                                            eprintln!("{} {}: {}", "Error".red().bold(), path, e)
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
            }
        }
        Commands::Gitai { actions } => match actions {
            GitAIActions::Commit {
                provider: provider_override,
                install,
                uninstall,
            } => {
                let repo = git2::Repository::open_from_env()?;
                let git_dir = repo.path();
                let hooks_dir = git_dir.join("hooks");

                if install {
                    return install_git_hook(&hooks_dir);
                }

                if uninstall {
                    return remove_git_hook(&hooks_dir);
                }

                let mut config = GitAIConfig::load().await?;
                if let Some(provider_name) = provider_override {
                    config.provider = provider_name;
                }

                let provider = create_provider(&config)?;

                let commit_msg = spinner::new(|| async {
                    let diff = git_diff();
                    provider.generate_commit_message(&diff?).await
                })
                .await?;

                println!(
                    "{}\n{}",
                    "Generated commit message:".green().bold(),
                    commit_msg
                );
            }
            GitAIActions::Config { action } => match action {
                GitAIConfigActions::Set { key, value } => {
                    let mut config = GitAIConfig::load().await?;
                    config.set(&key, &value)?;
                    config.save().await?;
                    println!("{} {} = {}", "Set:".green().bold(), key, value);
                }
                GitAIConfigActions::Get { key } => {
                    let config = GitAIConfig::load().await?;
                    if let Some(value) = config.get(&key) {
                        println!("{}", value);
                    } else {
                        println!("{} Key not found: {}", "Error:".red().bold(), key);
                    }
                }
                GitAIConfigActions::List => {
                    let config = GitAIConfig::load().await?;
                    println!("{}", "Configuration:".green().bold());
                    config.list().iter().for_each(|(k, v)| {
                        println!("{} = {}", k, v);
                    });
                }
            },
        },
        Commands::Completion { shell } => print_completions(shell, &mut Shelf::command())?,
    }
    Ok(())
}
