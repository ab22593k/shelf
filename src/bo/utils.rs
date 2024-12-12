use crate::bo::Bo;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};
use tokio::fs::metadata;
use walkdir::WalkDir;

/// Lists tracked files.
pub(crate) async fn handle_bo_list(conn: &Connection, modified_only: bool) -> Result<()> {
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
        println!("{}", "No tracked files found.".yellow());
        return Ok(());
    }

    println!("{}", "Tracked files:".green().bold());

    let mut modified_files_found = false;
    for (path, content, timestamp) in files {
        let path_buf = PathBuf::from(&path);

        if modified_only {
            if is_file_modified(&path_buf, timestamp).await? {
                modified_files_found = true;
            } else {
                /* Skip unmodified files */
                continue;
            }
        } else {
            /* set to true to avoid the "No modified files found"
            message if modified_only is false */
            modified_files_found = true;
        }

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

    if modified_only && !modified_files_found {
        println!("{}", "No modified tracked files found.".yellow());
    }

    Ok(())
}

async fn is_file_modified(path: &Path, db_timestamp: i64) -> Result<bool> {
    let metadata = metadata(path)
        .await
        .with_context(|| format!("Failed to get metadata for: {}", path.display()))?;
    let file_timestamp = metadata
        .modified()
        .with_context(|| format!("Failed to get modified time for: {}", path.display()))?
        .duration_since(UNIX_EPOCH)
        .context("System time before Unix epoch")?
        .as_secs() as i64;
    Ok(file_timestamp > db_timestamp)
}

/// Untracks files.
pub(crate) async fn handle_bo_untrack(
    conn: &Connection,
    recursive: bool,
    paths: Vec<PathBuf>,
) -> Result<()> {
    for base_path in paths {
        let paths_to_untrack = if recursive && base_path.is_dir() {
            WalkDir::new(&base_path)
                .follow_links(true)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .map(|e| e.path().to_path_buf())
                .collect()
        } else {
            vec![base_path]
        };

        for path_to_untrack in paths_to_untrack {
            if let Ok(dotconf) = Bo::select(conn, &path_to_untrack).await {
                Bo::remove(conn, &path_to_untrack).await?;
                println!("{} {}", "Removed:".green().bold(), dotconf.path.display());
            } else {
                eprintln!(
                    "{} No such file: {:?}",
                    "Error:".red().bold(),
                    path_to_untrack
                );
            }
        }
    }
    Ok(())
}

/// Tracks files.
pub(crate) async fn handle_bo_track(
    conn: &Connection,
    recursive: bool,
    restore: bool,
    paths: Vec<PathBuf>,
) -> Result<()> {
    for base_path in paths {
        let paths_to_track = if recursive && base_path.is_dir() {
            WalkDir::new(&base_path)
                .follow_links(true)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .map(|e| e.path().to_path_buf())
                .collect()
        } else {
            vec![base_path]
        };

        for path_to_track in paths_to_track {
            if restore {
                if let Ok(mut dotconf) = Bo::select(conn, &path_to_track).await {
                    dotconf.restore(conn).await?;
                    println!("{} {:?}", "Restored:".green().bold(), path_to_track);
                }
            } else if let Ok(mut dotconf) = Bo::from_file(&path_to_track).await {
                dotconf.insert(conn).await?;
                println!("{} {:?}", "Tracked:".green().bold(), path_to_track);
            }
        }
    }
    Ok(())
}

/// Helper function to format timestamps.
fn format_timestamp(timestamp: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_secs(timestamp as u64))
}
