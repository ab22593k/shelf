mod app;
mod config;
mod dotconf;

use anyhow::Result;
use app::{Commands, DotconfActions, Shelf};
use chrono::{DateTime, Utc};
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Generator};
use colored::*;
use dotconf::suggest::Suggestions;
use dotconf::Dotconf;
use rusqlite::Connection;

use std::time::SystemTime;
use std::{io, path::PathBuf};
use walkdir::WalkDir;

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) {
    let bin_name = cmd.get_bin_name().unwrap_or("slf").to_string();
    generate::<G, _>(gen, cmd, bin_name, &mut io::stdout());
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

    let db_path = match std::env::var("XDG_CONFIG_HOME") {
        Ok(path) => PathBuf::from(path),
        Err(_) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"),
    }
    .join("shelf")
    .join("dotconf.db");

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(db_path.clone())?;
    init_db(&conn).await?;

    // Ensure connection is properly initialized
    let conn = Connection::open(db_path.clone())?;
    init_db(&conn).await?;

    // Create prepared statements for common operations
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
                   PRAGMA journal_mode = WAL;",
    )?;

    match cli.command {
        Commands::Dotconf { actions } => {
            match actions {
                DotconfActions::List => {
                    let conn = rusqlite::Connection::open(db_path)?;

                    // Query all tracked files
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
                        let modified = DateTime::<Utc>::from(
                            SystemTime::UNIX_EPOCH
                                + std::time::Duration::from_secs(timestamp as u64),
                        );
                        println!("{}: {}", "Path".blue().bold(), path);
                        println!(
                            "{}: {}",
                            "Modified".cyan().bold(),
                            modified.format("%Y-%m-%d %H:%M:%S UTC")
                        );
                        println!("{}: {} bytes", "Size".magenta().bold(), content.len());
                        println!("{}", "=================".bright_black());
                    }
                }

                DotconfActions::Remove { recursive, paths } => {
                    let conn = Connection::open(&db_path)?;
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
                            println!(
                                "{} No such dotconf found: {:?}",
                                "Error:".red().bold(),
                                base_path
                            );
                        }
                    }
                }
                DotconfActions::Copy {
                    recursive,
                    backload,
                    paths,
                } => {
                    let conn = Connection::open(&db_path)?;

                    for base_path in paths {
                        if recursive && base_path.is_dir() {
                            for entry in WalkDir::new(&base_path)
                                .follow_links(true)
                                .into_iter()
                                .filter_map(|e| e.ok())
                            {
                                let path = entry.path();
                                if path.is_file() {
                                    if backload {
                                        if let Ok(mut dotconf) = Dotconf::select(&conn, path).await
                                        {
                                            dotconf.backload(&conn).await?;
                                            println!(
                                                "{} {:?}",
                                                "Successfully backloaded".green().bold(),
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
                        } else if backload {
                            if let Ok(mut dotconf) = Dotconf::select(&conn, &base_path).await {
                                dotconf.backload(&conn).await?;
                                println!(
                                    "{} {:?}",
                                    "Successfully backloaded".green().bold(),
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
                                    println!(
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
                    let conn = Connection::open(&db_path)?;
                    let suggestions = Suggestions::default();

                    if interactive {
                        match suggestions.interactive_selection() {
                            Ok(selected) => {
                                for path in selected {
                                    let expanded_path = shellexpand::tilde(&path).to_string();
                                    match Dotconf::from_file(expanded_path).await {
                                        Ok(mut dotconf) => {
                                            if let Err(e) = dotconf.insert(&conn).await {
                                                println!(
                                                    "{} {}: {}",
                                                    "Failed".red().bold(),
                                                    path,
                                                    e
                                                );
                                            } else {
                                                println!("{} {}", "Added".green().bold(), path);
                                            }
                                        }
                                        Err(e) => {
                                            println!("{} {}: {}", "Error".red().bold(), path, e)
                                        }
                                    }
                                }
                            }
                            Err(e) => println!("{} {}", "Selection failed:".red().bold(), e),
                        }
                    } else {
                        suggestions.print_suggestions();
                    }
                }
            }
        }
        Commands::Gitai => {
            todo!()
        }
        Commands::Completion { shell } => {
            let mut cmd = Shelf::command();
            print_completions(shell, &mut cmd);

            return Ok(());
        }
    }
    Ok(())
}
