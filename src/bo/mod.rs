pub mod suggest;
pub mod utils;

use anyhow::{Context, Result};
use rusqlite::Connection;
use tokio::fs;

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Represents a system configuration file bookmark that tracks files in your
/// home directory (or anywhere else) that need to be managed and kept in sync.
///
/// # Fields
///
/// * `path` - The filesystem path to the configuration file
/// * `content` - The current file contents as a string
/// * `inserted` - When this file was last synchronized
#[derive(Debug, Clone)]
pub struct Bo {
    /// Filesystem path to the tracked configuration file
    pub path: PathBuf,
    /// Current contents of the file as a string
    pub content: String,
    /// Timestamp when the file was last synchronized
    inserted: SystemTime,
}

impl Bo {
    pub const fn new(path: PathBuf, content: String, inserted: SystemTime) -> Self {
        Self {
            path,
            content,
            inserted,
        }
    }

    /// Creates a new `Bo` instance from a file.
    pub async fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        let metadata = fs::metadata(path)
            .await
            .with_context(|| format!("Failed to read metadata for: {}", path.display()))?;

        let last_modified = metadata
            .modified()
            .with_context(|| format!("Failed to get modification time for: {}", path.display()))?;

        Ok(Self::new(path.to_path_buf(), content, last_modified))
    }

    /// Retrieves a `Bo` from the database.
    pub async fn select(conn: &Connection, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut stmt =
            conn.prepare("SELECT content, last_modified FROM dotconf WHERE path = ?1")?;

        let (content, last_modified): (String, i64) = stmt
            .query_row([path.to_string_lossy()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;

        let inserted =
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(last_modified as u64);

        Ok(Self::new(path.to_path_buf(), content, inserted))
    }

    /// Inserts or updates a `Bo` in the database.
    pub async fn insert(&mut self, conn: &Connection) -> Result<()> {
        let unix_timestamp = self
            .inserted
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("System time before Unix epoch")? // More specific error
            .as_secs();

        conn.execute(
            "INSERT OR REPLACE INTO dotconf (path, content, last_modified) VALUES (?1, ?2, ?3)",
            (
                self.path.to_string_lossy(),
                &self.content,
                unix_timestamp as i64,
            ),
        )?;

        Ok(())
    }

    /// Removes a `Dotfile` from the database based on the given file path.
    pub async fn remove<T>(conn: &Connection, path: T) -> Result<()>
    where
        T: AsRef<Path>,
    {
        let path = path.as_ref();
        conn.execute(
            "DELETE FROM dotconf WHERE path = ?1",
            [path.to_string_lossy()],
        )?;
        Ok(())
    }

    /// Restores `Bo` content from the database to the file system.
    pub async fn restore(&mut self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("SELECT content FROM dotconf WHERE path = ?1")?;

        self.content = stmt
            .query_row([self.path.to_string_lossy()], |row| row.get(0))
            .with_context(|| {
                format!(
                    "Failed to read content from database for: {}",
                    self.path.display()
                )
            })?;

        fs::write(&self.path, &self.content)
            .await
            .with_context(|| format!("Failed to write file: {}", self.path.display()))?;

        let metadata = fs::metadata(&self.path)
            .await
            .with_context(|| format!("Failed to read metadata for: {}", self.path.display()))?;

        self.inserted = metadata.modified().with_context(|| {
            format!(
                "Failed to get modification time for: {}",
                self.path.display()
            )
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use tokio::fs;

    use super::*;
    use std::time::Duration;

    async fn setup_db() -> Result<rusqlite::Connection> {
        let conn = rusqlite::Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dotconf (
                path TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                last_modified INTEGER NOT NULL
            )",
            [],
        )?;

        Ok(conn)
    }

    #[tokio::test]
    async fn test_new() -> Result<()> {
        let path = PathBuf::from("test/path");
        let content = "test content".to_string();
        let time = SystemTime::now();

        let conf = Bo::new(path.clone(), content.clone(), time);

        assert_eq!(conf.path, path, "Path should match exactly");
        assert_eq!(conf.content, content, "Content should match exactly");
        assert_eq!(conf.inserted, time, "Timestamp should match exactly");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_path() -> Result<()> {
        let path = PathBuf::from("test/path");
        let conf = Bo::new(path.clone(), "content".to_string(), SystemTime::now());

        assert_eq!(conf.path, path, "get_path should return the original path");
        Ok(())
    }

    #[tokio::test]
    async fn test_select_from() -> Result<()> {
        let conn = setup_db().await?;
        let test_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1630000000);

        // Insert test data
        conn.execute(
            "INSERT INTO dotconf (path, content, last_modified) VALUES (?1, ?2, ?3)",
            ["test/path", "test content", &1630000000.to_string()],
        )?;

        let result = Bo::select(&conn, "test/path").await?;

        assert_eq!(result.path.to_string_lossy(), "test/path");
        assert_eq!(result.content, "test content");
        assert_eq!(result.inserted, test_time);

        // Test non-existent path
        assert!(Bo::select(&conn, "nonexistent").await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_insert_into() -> Result<()> {
        let conn = setup_db().await?;
        let test_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1630000000);

        let mut conf = Bo::new(
            PathBuf::from("test/path"),
            "initial content".to_string(),
            test_time,
        );

        // Test initial insert
        conf.insert(&conn).await?;

        // Verify insert
        let saved = Bo::select(&conn, "test/path").await?;
        assert_eq!(saved.content, "initial content");

        // Test update
        conf.content = "updated content".to_string();
        conf.insert(&conn).await?;

        // Verify update
        let updated = Bo::select(&conn, "test/path").await?;
        assert_eq!(updated.content, "updated content");

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_from() -> Result<()> {
        let conn = setup_db().await?;

        // Insert test data
        conn.execute(
            "INSERT INTO dotconf (path, content, last_modified) VALUES (?1, ?2, ?3)",
            ["test/path", "test content", &1630000000.to_string()],
        )?;

        // Verify insertion
        assert!(Bo::select(&conn, "test/path").await.is_ok());

        // Test deletion
        Bo::remove(&conn, "test/path").await?;

        // Verify deletion
        assert!(Bo::select(&conn, "test/path").await.is_err());

        // Test deleting non-existent entry (should not error)
        assert!(Bo::remove(&conn, "nonexistent").await.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_timestamp_handling() -> Result<()> {
        let conn = setup_db().await?;
        let test_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1630000000);

        let mut conf = Bo::new(PathBuf::from("test/path"), "content".to_string(), test_time);

        conf.insert(&conn).await?;

        let loaded = Bo::select(&conn, "test/path").await?;

        let time_diff = loaded
            .inserted
            .duration_since(test_time)
            .unwrap_or_default()
            .as_secs();

        assert!(
            time_diff < 1,
            "Timestamp should be preserved within 1 second accuracy"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_operations() -> Result<()> {
        let conn = setup_db().await?;
        let test_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1630000000);

        // Insert multiple test entries
        let paths = ["test/path1", "test/path2", "test/path3"];
        for path in paths.iter() {
            let mut conf = Bo::new(
                PathBuf::from(path),
                format!("content for {}", path),
                test_time,
            );
            conf.insert(&conn).await?;
        }

        // Verify all entries were inserted
        for path in paths.iter() {
            let conf = Bo::select(&conn, path).await?;
            assert_eq!(conf.content, format!("content for {}", path));
        }

        // Delete multiple entries
        for path in &paths[0..2] {
            Bo::remove(&conn, path).await?;
        }

        // Verify deleted entries are gone
        for path in &paths[0..2] {
            assert!(Bo::select(&conn, path).await.is_err());
        }

        // Verify remaining entry still exists
        let remaining = Bo::select(&conn, &paths[2]).await?;
        assert_eq!(remaining.content, format!("content for {}", paths[2]));

        Ok(())
    }

    #[tokio::test]
    async fn test_directory_operations() -> Result<()> {
        let conn = setup_db().await?;
        let test_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1630000000);

        // Create test entries simulating files in directories
        let dir_paths = [
            "test/dir1/file1",
            "test/dir1/file2",
            "test/dir1/subdir/file3",
            "test/dir2/file1",
        ];

        for path in dir_paths.iter() {
            let mut conf = Bo::new(
                PathBuf::from(path),
                format!("content for {}", path),
                test_time,
            );
            conf.insert(&conn).await?;
        }

        // Verify all entries exist
        for path in dir_paths.iter() {
            let conf = Bo::select(&conn, path).await?;
            assert_eq!(conf.content, format!("content for {}", path));
        }

        // Delete entire directory
        for entry in dir_paths.iter().filter(|p| p.starts_with("test/dir1")) {
            Bo::remove(&conn, entry).await?;
        }

        // Verify dir1 entries are deleted
        for entry in dir_paths.iter().filter(|p| p.starts_with("test/dir1")) {
            assert!(Bo::select(&conn, entry).await.is_err());
        }

        // Verify dir2 entry still exists
        let remaining = Bo::select(&conn, "test/dir2/file1").await?;
        assert_eq!(remaining.content, "content for test/dir2/file1");

        Ok(())
    }

    #[tokio::test]
    async fn test_restore() -> Result<()> {
        let conn = setup_db().await?;
        let test_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1630000000);
        let test_path = std::env::temp_dir().join("test_restore");

        // Ensure test file doesn't exist
        if test_path.exists() {
            fs::remove_file(&test_path).await?;
        }

        // Create initial config and save to DB
        let initial_content = "initial content".to_string();
        let mut conf = Bo::new(test_path.clone(), initial_content.clone(), test_time);
        conf.insert(&conn).await?;

        // Write different content to file
        let file_content = "file content".to_string();
        fs::write(&test_path, &file_content).await?;

        // Restore should restore DB content to file
        conf.restore(&conn).await?;

        // Verify file content matches DB content
        let file_content = fs::read_to_string(&test_path).await?;
        assert_eq!(file_content, initial_content);

        // Clean up
        fs::remove_file(&test_path).await?;

        Ok(())
    }
}
