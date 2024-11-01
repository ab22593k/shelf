pub mod suggest;

use anyhow::{anyhow, Result};

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug, Clone)]
pub struct Dotconf {
    path: PathBuf,
    content: String,
    last_modified: SystemTime,
}

impl Dotconf {
    pub fn new(path: PathBuf, content: String, last_modified: SystemTime) -> Self {
        Self {
            path,
            content,
            last_modified,
        }
    }

    pub fn get_path(self) -> PathBuf {
        self.path
    }

    pub async fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| anyhow!("Failed to read file {}: {}", path.display(), e))?;

        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| anyhow!("Failed to read metadata for {}: {}", path.display(), e))?;

        let last_modified = metadata.modified().map_err(|e| {
            anyhow!(
                "Failed to get modification time for {}: {}",
                path.display(),
                e
            )
        })?;

        Ok(Self::new(path.to_path_buf(), content, last_modified))
    }

    pub async fn select_from(conn: &rusqlite::Connection, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        let mut stmt =
            conn.prepare("SELECT content, last_modified FROM dotconf WHERE path = ?1")?;

        let (content, last_modified): (String, i64) = stmt
            .query_row([path.to_string_lossy().to_string()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;

        let last_modified =
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(last_modified as u64);

        Ok(Self::new(path.to_path_buf(), content, last_modified))
    }

    pub async fn insert_into(&mut self, conn: &rusqlite::Connection) -> Result<()> {
        let unix_timestamp = self
            .last_modified
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        conn.execute(
            "INSERT OR REPLACE INTO dotconf (path, content, last_modified) VALUES (?1, ?2, ?3)",
            (
                self.path.to_string_lossy().to_string(),
                &self.content,
                unix_timestamp as i64,
            ),
        )?;

        Ok(())
    }

    pub async fn delete_from(conn: &rusqlite::Connection, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        conn.execute(
            "DELETE FROM dotconf WHERE path = ?1",
            [path.to_string_lossy().to_string()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

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

        let conf = Dotconf::new(path.clone(), content.clone(), time);

        assert_eq!(conf.path, path, "Path should match exactly");
        assert_eq!(conf.content, content, "Content should match exactly");
        assert_eq!(conf.last_modified, time, "Timestamp should match exactly");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_path() -> Result<()> {
        let path = PathBuf::from("test/path");
        let conf = Dotconf::new(path.clone(), "content".to_string(), SystemTime::now());

        assert_eq!(
            conf.get_path(),
            path,
            "get_path should return the original path"
        );
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

        let result = Dotconf::select_from(&conn, "test/path").await?;

        assert_eq!(result.path.to_string_lossy(), "test/path");
        assert_eq!(result.content, "test content");
        assert_eq!(result.last_modified, test_time);

        // Test non-existent path
        assert!(Dotconf::select_from(&conn, "nonexistent").await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_insert_into() -> Result<()> {
        let conn = setup_db().await?;
        let test_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1630000000);

        let mut conf = Dotconf::new(
            PathBuf::from("test/path"),
            "initial content".to_string(),
            test_time,
        );

        // Test initial insert
        conf.insert_into(&conn).await?;

        // Verify insert
        let saved = Dotconf::select_from(&conn, "test/path").await?;
        assert_eq!(saved.content, "initial content");

        // Test update
        conf.content = "updated content".to_string();
        conf.insert_into(&conn).await?;

        // Verify update
        let updated = Dotconf::select_from(&conn, "test/path").await?;
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
        assert!(Dotconf::select_from(&conn, "test/path").await.is_ok());

        // Test deletion
        Dotconf::delete_from(&conn, "test/path").await?;

        // Verify deletion
        assert!(Dotconf::select_from(&conn, "test/path").await.is_err());

        // Test deleting non-existent entry (should not error)
        assert!(Dotconf::delete_from(&conn, "nonexistent").await.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_timestamp_handling() -> Result<()> {
        let conn = setup_db().await?;
        let test_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1630000000);

        let mut conf = Dotconf::new(PathBuf::from("test/path"), "content".to_string(), test_time);

        conf.insert_into(&conn).await?;

        let loaded = Dotconf::select_from(&conn, "test/path").await?;

        let time_diff = loaded
            .last_modified
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
            let mut conf = Dotconf::new(
                PathBuf::from(path),
                format!("content for {}", path),
                test_time,
            );
            conf.insert_into(&conn).await?;
        }

        // Verify all entries were inserted
        for path in paths.iter() {
            let conf = Dotconf::select_from(&conn, path).await?;
            assert_eq!(conf.content, format!("content for {}", path));
        }

        // Delete multiple entries
        for path in &paths[0..2] {
            Dotconf::delete_from(&conn, path).await?;
        }

        // Verify deleted entries are gone
        for path in &paths[0..2] {
            assert!(Dotconf::select_from(&conn, path).await.is_err());
        }

        // Verify remaining entry still exists
        let remaining = Dotconf::select_from(&conn, &paths[2]).await?;
        assert_eq!(remaining.content, format!("content for {}", paths[2]));

        Ok(())
    }
}
