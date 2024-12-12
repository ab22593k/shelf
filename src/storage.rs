use anyhow::{Context, Result};
use rusqlite::Connection;

pub const BO_STORAGE_FILENAME: &str = "bo.sqlite";

pub async fn establish_connection() -> Result<Connection> {
    let storage_file = directories::BaseDirs::new()
        .map(|base| base.data_dir().join("shelf").join(BO_STORAGE_FILENAME))
        .context("Could not create `shelf` data directory")?;

    let conn = Connection::open(&storage_file).context("Failed to open database")?;

    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;",
    )
    .context("Failed to set pragmas")?;

    Ok(conn)
}

pub async fn initialize_database(conn: &Connection) -> Result<()> {
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
