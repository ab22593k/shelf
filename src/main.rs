#![allow(clippy::nonminimal_bool)]

mod config;
mod dotconf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use colored::*;
use dotconf::suggest::Suggestions;
use dotconf::Dotconf;
use rusqlite::Connection;

use std::time::SystemTime;
use std::{io, path::PathBuf};

// Version and crate information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");

#[derive(Parser)]
#[command(author, about, long_about = None ,version)]
#[command(arg_required_else_help = true)]
pub struct Shelf {
    #[command(subcommand)]
    pub command: Actions,
}

#[derive(Subcommand)]
pub enum Actions {
    #[command(name = "ls", about = "List all currently tracked dotconf[s]")]
    List,

    #[command(name = "rm", about = "Remove dotconf[s] from management")]
    Remove { path: PathBuf },

    #[command(name = "cp", about = "Create a dotconf[s] copy")]
    Copy { path: PathBuf },

    #[command(about = "Suggest commonly used dotconf[s] cross diffrent OS's")]
    Suggest {
        #[arg(short, long)]
        interactive: bool,
    },

    #[command(about = "Generate shell completion scripts")]
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
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
        Actions::List => {
            let conn = rusqlite::Connection::open(db_path)?;

            // Query all tracked files
            let mut stmt = conn.prepare("SELECT path, content, last_modified FROM dotconf")?;
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
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64),
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
        Actions::Remove { path } => {
            let conn = Connection::open(&db_path)?;
            // Get the dotconf info before deletion for display
            if let Ok(dotconf) = Dotconf::select_from(&conn, &path).await {
                Dotconf::delete_from(&conn, &path).await?;
                println!(
                    "{} {}",
                    "Removed:".green().bold(),
                    dotconf.get_path().display()
                );
            } else {
                println!(
                    "{} No such dotconf found: {:?}",
                    "Error:".red().bold(),
                    path
                );
            }
        }
        Actions::Copy { path } => {
            let conn = Connection::open(&db_path)?;

            // Try to create new dotconf from file
            match Dotconf::from_file(&path).await {
                Ok(mut dotconf) => {
                    dotconf.insert_into(&conn).await?;
                    println!("{} {:?}", "Successfully copied".green().bold(), path);
                }
                Err(e) => {
                    println!("{} {:?}: {}", "Failed to copy".red().bold(), path, e);
                }
            }
        }
        Actions::Suggest { interactive } => {
            let conn = Connection::open(&db_path)?;
            let suggestions = Suggestions::default();

            if interactive {
                match suggestions.interactive_selection() {
                    Ok(selected) => {
                        for path in selected {
                            let expanded_path = shellexpand::tilde(&path).to_string();
                            match Dotconf::from_file(expanded_path).await {
                                Ok(mut dotconf) => {
                                    if let Err(e) = dotconf.insert_into(&conn).await {
                                        println!("{} {}: {}", "Failed".red().bold(), path, e);
                                    } else {
                                        println!("{} {}", "Added".green().bold(), path);
                                    }
                                }
                                Err(e) => println!("{} {}: {}", "Error".red().bold(), path, e),
                            }
                        }
                    }
                    Err(e) => println!("{} {}", "Selection failed:".red().bold(), e),
                }
            } else {
                suggestions.print_suggestions();
            }
        }
        Actions::Completion { shell } => {
            let mut cmd = Shelf::command();
            print_completions(shell, &mut cmd);
            return Ok(());
        }
    }

    Ok(())
}
