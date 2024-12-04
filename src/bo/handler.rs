use crate::bo::Books;

use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::Colorize;
use rusqlite::Connection;
use walkdir::WalkDir;

use std::{path::PathBuf, time::UNIX_EPOCH};

pub(crate) async fn handle_fs_list(conn: &Connection, modified: bool) -> Result<()> {
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

    let mut modified_files_found = false;
    for (path, content, timestamp) in files {
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
        modified_files_found = true;
        let inserted = format_timestamp(timestamp);
        println!("{}: {}", "Path".blue().bold(), path);
        println!(
            "{}: {}",
            "Inserted".cyan().bold(),
            inserted.format("%Y-%m-%d %H:%M:%S UTC")
        );
        println!("{}: {} bytes", "Size".magenta().bold(), content.len());
        println!("*{}", "-".repeat(5).bright_black());
    }

    if modified && !modified_files_found {
        println!("{}", "No modified tracked files found.".yellow());
    }

    Ok(())
}

pub(crate) async fn handle_fs_untrack(
    conn: &Connection,
    recursive: bool,
    paths: Vec<PathBuf>,
) -> Result<()> {
    for base_path in paths {
        if recursive && base_path.is_dir() {
            for entry in WalkDir::new(&base_path)
                .follow_links(true)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.path().is_file())
            {
                if let Ok(dotconf) = Books::select(conn, entry.path()).await {
                    Books::remove(conn, entry.path()).await?;
                    println!(
                        "{} {}",
                        "Removed:".green().bold(),
                        dotconf.get_path().display()
                    );
                }
            }
        } else if let Ok(dotconf) = Books::select(conn, &base_path).await {
            Books::remove(conn, &base_path).await?;
            println!(
                "{} {}",
                "Removed:".green().bold(),
                dotconf.get_path().display()
            );
        } else {
            eprintln!("{} No such file: {:?}", "Error:".red().bold(), base_path);
        }
    }
    Ok(())
}

pub(crate) async fn handle_fs_track(
    conn: &Connection,
    recursive: bool,
    restore: bool,
    paths: Vec<PathBuf>,
) -> Result<()> {
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
                    if let Ok(mut dotconf) = Books::select(conn, path).await {
                        dotconf.restore(conn).await?;
                        println!("{} {:?}", "Restored:".green().bold(), path);
                    }
                } else if let Ok(mut dotconf) = Books::from_file(path).await {
                    dotconf.insert(conn).await?;
                    println!("{} {:?}", "Tracked:".green().bold(), path);
                }
            }
        } else if restore {
            if let Ok(mut dotconf) = Books::select(conn, &base_path).await {
                dotconf.restore(conn).await?;
                println!("{} {:?}", "Restored:".green().bold(), base_path);
            }
        } else if let Ok(mut dotconf) = Books::from_file(&base_path).await {
            dotconf.insert(conn).await?;
            println!("{} {:?}", "Tracked:".green().bold(), base_path);
        }
    }
    Ok(())
}

// Helper function to format timestamps
fn format_timestamp(timestamp: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from(UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64))
}
