//! This module provides comprehensive functionality for managing dotfiles.
//!
//! Key features include:
//! - Adding and removing dotfiles to/from the collection
//! - Copying dotfiles to a target directory
//! - Maintaining an index of dotfiles with metadata (inode, version)
//! - Handling file conflicts and creating backups
//! - Serialization and deserialization of the dotfile collection
//!
//! The module uses asynchronous operations for improved performance and
//! provides detailed error handling and user feedback through colored output.

use anyhow::{anyhow, Context, Result};
use colored::*;
use serde::{Deserialize, Serialize};
use tokio::fs;

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Represents a collection of dotfiles with their metadata and operations.
///
/// This struct is the main container for managing dotfiles. It holds a map of
/// dotfile entries, the target directory for copying dotfiles, the configuration
/// directory, and the remote host configuration.
///
/// # Fields
///
/// * `dotfiles` - A HashMap containing Index instances, keyed by their names.
/// * `target_directory` - The directory where copies of the dotfiles will be created.
/// * `config_dir` - The directory where configuration files are stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dotfiles {
    pub dotfiles: HashMap<String, Index>,
    pub target_directory: PathBuf,
    pub config_dir: PathBuf,
}

/// Represents a single dotfile entry with its source path and metadata.
///
/// This struct holds the information for an individual dotfile, including its
/// source path, inode number, and version.
///
/// # Fields
///
/// * `source` - The source path of the dotfile.
/// * `inode` - The inode number of the dotfile for change detection.
/// * `version` - The version number of the dotfile entry, incremented on changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub source: PathBuf,
    pub inode: u64,
}

impl Index {
    pub fn new(source: PathBuf, inode: u64) -> Self {
        Self { source, inode }
    }
}

impl Dotfiles {
    /// Loads the Dotfiles collection from a JSON file or creates a new one if it doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `config_dir` - The directory where the configuration file is stored.
    /// * `target_directory` - The directory where dotfile copies will be created.
    ///
    /// # Returns
    ///
    /// A Result containing the Dotfiles instance or an error.
    pub async fn load(config_dir: PathBuf, target_directory: PathBuf) -> Result<Self> {
        let index_path = config_dir.join("index.json");
        if index_path.exists() {
            let json = fs::read_to_string(&index_path).await?;
            let mut dotfiles: Dotfiles = serde_json::from_str(&json)?;
            dotfiles.config_dir = config_dir;
            dotfiles.target_directory = target_directory;
            Ok(dotfiles)
        } else {
            Self::new(config_dir, target_directory).await
        }
    }

    /// Creates a new Dotfiles instance.
    ///
    /// # Arguments
    ///
    /// * `config_dir` - The directory where the configuration file will be stored.
    /// * `target_directory` - The directory where dotfile copies will be created.
    ///
    /// # Returns
    ///
    /// A Result containing the new Dotfiles instance or an error.
    pub async fn new(config_dir: PathBuf, target_directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&target_directory)
            .await
            .context("Failed to create target directory")?;

        Ok(Self {
            dotfiles: HashMap::new(),
            target_directory,
            config_dir,
        })
    }

    /// Adds a single dotfile to the collection.
    ///
    /// # Arguments
    ///
    /// * `file` - The path to the dotfile to be added.
    ///
    /// # Returns
    ///
    /// A Result indicating success or an error.
    pub async fn add<P: AsRef<Path>>(&mut self, file: P) -> Result<()> {
        let source = file.as_ref().to_path_buf();
        let name = source
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .ok_or_else(|| anyhow!("{} Invalid file name", "Error:".red().bold()))?
            .to_string();

        if !source.exists() {
            return Err(anyhow!(
                "{} Source file does not exist: {}",
                "Error:".red().bold(),
                source.display().to_string().cyan()
            ));
        }
        let source = fs::canonicalize(&source).await.with_context(|| {
            format!(
                "{} Failed to canonicalize source path: {}",
                "Error:".red().bold(),
                source.display().to_string().cyan()
            )
        })?;

        let metadata = fs::metadata(&source).await?;
        let index = Index::new(source, metadata.ino());

        self.dotfiles.insert(name.clone(), index);
        println!(
            "{} Added dotfile: {}",
            "Success:".green().bold(),
            name.cyan()
        );

        self.save(&self.config_dir).await?;

        Ok(())
    }

    /// Removes multiple dotfiles from the collection.
    ///
    /// # Arguments
    ///
    /// * `fnames` - A slice of dotfile names to be removed.
    ///
    /// # Returns
    ///
    /// A vector of Results, one for each dotfile removal attempt.
    pub fn remove_multi(&mut self, fnames: &[&str]) -> Vec<Result<()>> {
        fnames
            .iter()
            .map(|&fname| self.remove_single(fname))
            .collect()
    }

    fn remove_single(&mut self, name: &str) -> Result<()> {
        self.dotfiles.remove(name).ok_or_else(|| {
            anyhow!(
                "{} Dotfile '{}' not found in the collection",
                "Error:".red().bold(),
                name.cyan()
            )
        })?;

        let target = self.target_directory.join(name);
        if target.exists() {
            std::fs::remove_file(&target).with_context(|| {
                format!(
                    "{} Failed to remove file for dotfile: {}",
                    "Error:".red().bold(),
                    name.cyan()
                )
            })?;
            println!(
                "{} Removed dotfile: {}",
                "Success:".green().bold(),
                name.cyan()
            );
        } else {
            println!(
                "{} Removed dotfile (no file found): {}",
                "Info:".blue().bold(),
                name.cyan()
            );
        }

        Ok(())
    }

    /// Prints a formatted list of all tracked dotfiles.
    pub fn print_list(&self) {
        println!("{}", "Tracked Dotfiles:".green().bold());
        if self.dotfiles.is_empty() {
            println!("  {}", "No dotfiles tracked.".yellow().italic());
        } else {
            for (name, entry) in self.dotfiles.iter() {
                println!(
                    "  {} {} {}",
                    "•".cyan().bold(),
                    name.cyan(),
                    format!("({})", entry.source.display()).dimmed()
                );
            }
        }
        println!(
            "{}",
            format!("Total: {} dotfiles", self.dotfiles.len())
                .blue()
                .bold()
        );
    }

    /// Copies all dotfiles to the target directory.
    ///
    /// # Returns
    ///
    /// A Result indicating success or an error.
    pub async fn copy(&self) -> Result<()> {
        println!(
            "{}",
            "Copying dotfiles to the output directory...".blue().bold()
        );

        let mut success_count = 0;
        let total_count = self.dotfiles.len();

        for (name, dotfile) in &self.dotfiles {
            let target_path = self.target_directory.join(name);

            if !dotfile.source.exists() {
                eprintln!(
                    "{} Source file does not exist: {}",
                    "Error:".red().bold(),
                    dotfile.source.display()
                );
                continue;
            }

            match self.copy_single_file(&dotfile.source, &target_path).await {
                Ok(_) => {
                    println!(
                        "{} Copied: {} -> {}",
                        "Success:".green().bold(),
                        dotfile.source.display(),
                        target_path.display()
                    );
                    success_count += 1;
                }
                Err(e) => {
                    eprintln!(
                        "{} Failed to copy {} to {}: {}",
                        "Error:".red().bold(),
                        dotfile.source.display(),
                        target_path.display(),
                        e
                    );
                }
            }
        }

        println!(
            "{} Copied {} out of {} dotfiles successfully.",
            "Summary:".blue().bold(),
            success_count,
            total_count
        );

        Ok(())
    }

    async fn copy_single_file(&self, source: &Path, target: &Path) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .await
                .context("Failed to create parent directory")?;
        }

        // Handle existing files, directories, or symlinks
        if target.exists() || fs::symlink_metadata(target).await.is_ok() {
            self.handle_existing_file(target).await?;
        }

        // Copy the file
        fs::copy(source, target)
            .await
            .context("Failed to copy file")?;

        Ok(())
    }

    async fn handle_existing_file(&self, path: &Path) -> Result<()> {
        let file_type = fs::symlink_metadata(path).await?.file_type();

        if file_type.is_symlink() {
            fs::remove_file(path)
                .await
                .context("Failed to remove existing symlink")?;
        } else if file_type.is_dir() {
            fs::remove_dir_all(path)
                .await
                .context("Failed to remove existing directory")?;
        } else {
            // Backup existing file
            let backup_path = path.with_extension("bak");
            fs::rename(path, &backup_path)
                .await
                .context("Failed to backup existing file")?;
            println!(
                "{} Backed up existing file: {} -> {}",
                "Info:".blue().bold(),
                path.display(),
                backup_path.display()
            );
        }

        Ok(())
    }

    /// Adds multiple dotfiles to the collection.
    ///
    /// This method takes an iterator of paths and attempts to add each one as a dotfile.
    /// It returns a vector of results, one for each file attempted to be added.
    ///
    /// # Arguments
    ///
    /// * `sources` - An iterator of path-like objects representing the dotfiles to be added.
    ///
    /// # Returns
    ///
    /// * `Vec<Result<(), anyhow::Error>>` - A vector of results, one for each file.
    pub async fn add_multi<P, I>(&mut self, sources: I) -> Vec<Result<(), anyhow::Error>>
    where
        P: AsRef<Path>,
        I: IntoIterator<Item = P>,
    {
        println!("{}", "Adding multiple dotfiles...".blue().bold());
        let mut results = Vec::new();
        for source in sources {
            let result = self.add(source).await;
            if let Err(ref e) = result {
                println!("{} {}", "Failed to add dotfile:".red().bold(), e);
            }
            results.push(result);
        }
        println!("{}", "Finished adding dotfiles".green().bold());
        results
    }

    /// Saves the current state of the Dotfiles collection to a JSON file.
    ///
    /// # Arguments
    ///
    /// * `index_path` - The path where the JSON file should be saved.
    ///
    /// # Returns
    ///
    /// A Result indicating success or an error.
    pub async fn save(&self, index_path: &Path) -> Result<()> {
        // Ensure the directory exists
        if let Some(parent) = index_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Append "index.json" if the path is a directory
        let file_path = if index_path.is_dir() {
            index_path.join("index.json")
        } else {
            index_path.to_path_buf()
        };

        let json = serde_json::to_string(&self)?;
        tokio::fs::write(file_path, json).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;
    use tokio::fs;

    use super::Dotfiles;

    async fn setup_test_environment() -> Result<(TempDir, Dotfiles)> {
        let temp_dir = TempDir::new()?;
        let config_dir = temp_dir.path().join("shelf");
        let target_dir = config_dir.join("dotfiles");
        fs::create_dir_all(&target_dir).await?;
        let absolute_target_dir = fs::canonicalize(&target_dir).await?;

        // Ensure the target directory is accessible
        fs::metadata(&absolute_target_dir).await?;

        let index = Dotfiles::new(config_dir.clone(), absolute_target_dir).await?;
        Ok((temp_dir, index))
    }

    async fn create_test_file(dir: &Path, name: &str, content: &str) -> Result<PathBuf> {
        let file_path = dir.join(name);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&file_path, content).await?;
        Ok(file_path)
    }

    async fn create_test_files(dir: &Path, files: &[(&str, &str)]) -> Result<Vec<PathBuf>> {
        let mut created_files = Vec::new();
        for (name, content) in files {
            let file_path = create_test_file(dir, name, content).await?;
            created_files.push(file_path);
        }
        Ok(created_files)
    }

    async fn add_test_files_to_index(index: &mut Dotfiles, files: &[PathBuf]) -> Result<()> {
        for file in files {
            index.add(file.to_str().unwrap()).await?;
        }
        Ok(())
    }
    async fn setup_test_index_with_files(
        files: &[(&str, &str)],
    ) -> Result<(TempDir, Dotfiles, Vec<PathBuf>)> {
        let (temp_dir, mut df) = setup_test_environment().await?;
        let created_files = create_test_files(temp_dir.path(), files).await?;
        add_test_files_to_index(&mut df, &created_files).await?;
        Ok((temp_dir, df, created_files))
    }

    #[tokio::test]
    async fn test_add_and_list_dotfile() -> Result<()> {
        let mut files = [
            (".testrc", "test content"),
            (".vimrc", "set nocompatible"),
            (".bashrc", "export PATH=$PATH:/usr/local/bin"),
        ];
        files.sort_by(|a, b| a.0.cmp(b.0));
        let (temp_dir, mut df, created_files) = setup_test_index_with_files(&files).await?;
        // let mut dotfiles = index;
        let mut dotfiles: Vec<_> = df.dotfiles.iter().collect();
        dotfiles.sort_by(|a, b| a.0.cmp(b.0));

        assert_eq!(
            dotfiles.len(),
            files.len(),
            "Expected {} dotfiles, found {}",
            files.len(),
            dotfiles.len()
        );

        for (i, (name, dotfile)) in dotfiles.iter().enumerate() {
            let expected_name = files[i].0;
            assert!(
                name.ends_with(expected_name),
                "Unexpected dotfile name: {}, expected to end with: {}",
                name,
                expected_name
            );

            let canonical_test_file = created_files[i]
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("Failed to canonicalize test file: {}", e))?;
            let canonical_dotfile_source = dotfile
                .source
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("Failed to canonicalize dotfile source: {}", e))?;

            assert_eq!(
                canonical_dotfile_source, canonical_test_file,
                "Paths don't match for {}: {:?} vs {:?}",
                name, canonical_dotfile_source, canonical_test_file
            );

            // Verify file content
            let content = fs::read_to_string(dotfile.source.clone())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to read dotfile content: {}", e))?;
            assert_eq!(
                content, files[i].1,
                "Content mismatch for {}: expected '{}', got '{}'",
                name, files[i].1, content
            );
        }
        // Test adding a file that already exists
        let file_path = created_files[0].to_str().unwrap().to_string();
        let result = df.add(&file_path).await;
        assert!(result.is_ok(), "Adding an existing dotfile should succeed");

        // Cleanup
        drop(df);
        assert!(
            temp_dir.path().exists(),
            "Temporary directory should still exist"
        );
        temp_dir
            .close()
            .map_err(|e| anyhow::anyhow!("Failed to close temporary directory: {}", e))?;

        Ok(())
    }

    #[tokio::test]
    async fn test_add_multiple_dotfiles() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;

        let files = [
            (".bashrc", "# Bash configuration"),
            (".vimrc", "\" Vim configuration"),
            (".gitconfig", "[user]\n\tname = Test User"),
        ];
        let created_files = create_test_files(temp_dir.path(), &files).await?;

        let results = index
            .add_multi(created_files.iter().map(|p| p.to_str().unwrap()))
            .await;

        assert_eq!(
            results.len(),
            files.len(),
            "Expected {} results, got {}",
            files.len(),
            results.len()
        );
        for result in &results {
            assert!(result.is_ok(), "Adding a dotfile failed: {:?}", result);
        }

        let dotfiles: Vec<_> = index.dotfiles.iter().collect();
        assert_eq!(
            dotfiles.len(),
            files.len(),
            "Expected {} dotfiles, found {}",
            files.len(),
            dotfiles.len()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_add_duplicate_dotfile() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let test_file = create_test_file(temp_dir.path(), ".testrc", "original content").await?;

        index.add(test_file.to_str().unwrap()).await?;
        let result = index.add(test_file.to_str().unwrap()).await;

        assert!(result.is_ok(), "Adding a duplicate dotfile should succeed");

        // Verify that the original file still exists and hasn't been modified
        assert!(test_file.exists(), "Original file should still exist");
        let content = fs::read_to_string(&test_file).await?;
        assert_eq!(
            content, "original content",
            "Original file content should be unchanged"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_add_multiple_dotfiles_at_once() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let files = [
            (".bashrc", "# Bash configuration"),
            (".vimrc", "\" Vim configuration"),
            (".gitconfig", "[user]\n\tname = Test User"),
        ];
        let created_files = create_test_files(temp_dir.path(), &files).await?;

        let results = index
            .add_multi(created_files.iter().map(|p| p.to_str().unwrap()))
            .await;

        assert_eq!(results.len(), 3, "Expected 3 results, one for each file");
        assert!(
            results.iter().all(|r| r.is_ok()),
            "All files should be added successfully"
        );

        let dotfiles = index.dotfiles.iter().collect::<Vec<_>>();

        assert_eq!(dotfiles.len(), 3, "Expected 3 dotfiles to be tracked");

        for (file, content) in files.iter() {
            let dotfile = dotfiles.iter().find(|(name, _)| name.ends_with(file));
            assert!(dotfile.is_some(), "Dotfile {} should be tracked", file);
            let (_, entry) = dotfile.unwrap();
            let file_content = fs::read_to_string(entry.source.clone()).await?;
            assert_eq!(
                file_content, *content,
                "File content should match for {}",
                file
            );
        }

        Ok(())
    }
    #[tokio::test]
    async fn test_link_creates_copies() -> Result<()> {
        let files = [(".testrc", "test content")];
        let (temp_dir, index, created_files) = setup_test_index_with_files(&files).await?;
        index.copy().await?;

        let target_dir = temp_dir.path().join("shelf").join("dotfiles");
        let copied_file = target_dir.join(".testrc");

        assert!(copied_file.exists(), "Copie file should exist after sync");
        assert!(
            !copied_file.is_symlink(),
            "Created file should not be a symlink"
        );

        let canonical_copied_file = copied_file.canonicalize()?;
        let canonical_created_file = created_files[0].canonicalize()?;
        assert_ne!(
            canonical_copied_file, canonical_created_file,
            "Copied file should not be the same as the original file"
        );

        // Verify original file still exists and contains the correct content
        assert!(
            created_files[0].exists(),
            "Original file should still exist"
        );
        let original_content = fs::read_to_string(&created_files[0]).await?;
        assert_eq!(
            original_content, "test content",
            "Original file content should be unchanged"
        );

        // Verify copied file content matches original file
        let copied_content = fs::read_to_string(&copied_file).await?;
        assert_eq!(
            copied_content, "test content",
            "Copied file content should match original file"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_returns_correct_info() -> Result<()> {
        let mut files = [
            (".bashrc", "# Bash configuration"),
            (".vimrc", "\" Vim configuration"),
            (".gitconfig", "[user]\n\tname = Test User"),
        ];
        files.sort_by(|a, b| a.0.cmp(b.0));
        let (temp_dir, index, created_files) = setup_test_index_with_files(&files).await?;

        let mut dotfiles: Vec<_> = index.dotfiles.iter().collect();
        dotfiles.sort_by(|a, b| a.0.cmp(b.0));

        assert_eq!(
            dotfiles.len(),
            files.len(),
            "Expected {} dotfiles, found {}",
            files.len(),
            dotfiles.len()
        );

        for (i, (name, dotfile)) in dotfiles.iter().enumerate() {
            let expected_name = files[i].0;
            assert!(
                name.ends_with(expected_name),
                "Unexpected dotfile name: {}, expected to end with: {}",
                name,
                expected_name
            );
            let canonical_source = dotfile.source.canonicalize()?;
            let canonical_created = created_files[i].canonicalize()?;
            assert_eq!(
                canonical_source, canonical_created,
                "Paths don't match for {}: {:?} vs {:?}",
                name, canonical_source, canonical_created
            );

            // Verify file content
            let content = fs::read_to_string(dotfile.source.clone()).await?;
            assert_eq!(
                content, files[i].1,
                "Content mismatch for {}: expected '{}', got '{}'",
                name, files[i].1, content
            );
        }

        // Test that all expected files are present
        let listed_names: Vec<_> = dotfiles.iter().map(|(name, _)| name).collect();
        for file in &files {
            assert!(
                listed_names.iter().any(|&name| name.ends_with(file.0)),
                "Expected file {} not found in listed dotfiles",
                file.0
            );
        }

        // Cleanup check
        drop(index);
        assert!(
            temp_dir.path().exists(),
            "Temporary directory should still exist"
        );
        temp_dir.close()?;

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_dotfile() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let test_file = create_test_file(temp_dir.path(), ".testrc", "test content").await?;
        index.add(test_file.to_str().unwrap()).await?;
        let remove_results = index.remove_multi(&[".testrc"]);
        assert!(
            remove_results.iter().all(|r| r.is_ok()),
            "Removing dotfile should succeed"
        );

        let dotfiles = index.dotfiles.iter().collect::<Vec<_>>();
        assert_eq!(dotfiles.len(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_multiple_dotfiles() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let files = [
            (".testrc1", "test content 1"),
            (".testrc2", "test content 2"),
            (".testrc3", "test content 3"),
        ];
        let created_files = create_test_files(temp_dir.path(), &files).await?;
        add_test_files_to_index(&mut index, &created_files).await?;

        // Remove two of the three dotfiles
        let result = index.remove_multi(&[".testrc1", ".testrc2"]);
        assert!(
            result.iter().all(|r| r.is_ok()),
            "Removing multiple dotfiles should succeed"
        );

        // Get the remaining dotfiles without cloning
        let remaining_dotfiles = index.dotfiles;
        assert_eq!(
            remaining_dotfiles.len(),
            1,
            "Should have one remaining dotfile"
        );
        assert!(
            remaining_dotfiles.contains_key(".testrc3"),
            "The remaining dotfile should be .testrc3"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_nonexistent_dotfiles() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let test_file = create_test_file(temp_dir.path(), ".testrc", "test content").await?;
        index.add(test_file.to_str().unwrap()).await?;
        let result = index.remove_multi(&[".testrc", "nonexistent1", "nonexistent2"]);

        assert_eq!(result.len(), 3, "Expected three results");
        assert!(
            result[0].is_ok(),
            "Removing existing dotfile should succeed"
        );
        assert!(result[1].is_err(), "Removing nonexistent1 should fail");
        assert!(result[2].is_err(), "Removing nonexistent2 should fail");

        let error_messages: Vec<String> = result
            .iter()
            .filter_map(|r| r.as_ref().err().map(|e| e.to_string().to_lowercase()))
            .collect();

        assert!(
            error_messages
                .iter()
                .any(|msg| msg.contains("not found") || msg.contains("doesn't exist")),
            "At least one error message should indicate that a dotfile was not found"
        );

        let remaining_dotfiles = index.dotfiles;
        assert_eq!(
            remaining_dotfiles.len(),
            0,
            "All existing dotfiles should be removed"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_dotfiles_partial_success() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let files = [
            (".testrc1", "test content 1"),
            (".testrc2", "test content 2"),
        ];
        let created_files = create_test_files(temp_dir.path(), &files).await?;
        add_test_files_to_index(&mut index, &created_files).await?;
        let result = index.remove_multi(&[".testrc1", ".testrc2", "nonexistent"]);

        assert_eq!(result.len(), 3, "Expected three results");
        assert!(result[0].is_ok(), "Removing .testrc1 should succeed");
        assert!(result[1].is_ok(), "Removing .testrc2 should succeed");
        assert!(result[2].is_err(), "Removing nonexistent should fail");

        let error_messages: Vec<String> = result
            .iter()
            .filter_map(|r| r.as_ref().err().map(|e| e.to_string().to_lowercase()))
            .collect();

        assert!(
            error_messages
                .iter()
                .any(|msg| msg.contains("not found") || msg.contains("doesn't exist")),
            "Error message should indicate that a dotfile was not found"
        );

        let remaining_dotfiles = index.dotfiles;
        assert_eq!(
            remaining_dotfiles.len(),
            0,
            "All existing dotfiles should be removed"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_sync_dotfiles() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let test_file1 = create_test_file(temp_dir.path(), ".testrc1", "test content 1").await?;
        let test_file2 = create_test_file(temp_dir.path(), ".testrc2", "test content 2").await?;
        let test_file3 = create_test_file(temp_dir.path(), ".testrc3", "test content 3").await?;

        index.add(test_file1.to_str().unwrap()).await?;
        index.add(test_file2.to_str().unwrap()).await?;
        index.add(test_file3.to_str().unwrap()).await?;

        index.copy().await?;

        let target_dir = temp_dir.path().join("shelf").join("dotfiles");

        for i in 1..=3 {
            let file_name = format!(".testrc{}", i);
            let copied_file = target_dir.join(&file_name);
            assert!(
                copied_file.exists(),
                "Copied file {} should exist",
                file_name
            );
            assert!(
                !copied_file.is_symlink(),
                "Copied file {} should not be a symlink",
                file_name
            );

            let content = fs::read_to_string(&copied_file).await?;
            assert_eq!(
                content,
                format!("test content {}", i),
                "Copied file {} content should match original",
                file_name
            );
        }
        let test_file2 = create_test_file(temp_dir.path(), ".testrc2", "test content 2").await?;

        index.add(&test_file1.to_str().unwrap()).await?;
        index.add(&test_file2.to_str().unwrap()).await?;

        index.copy().await?;

        let target_dir = temp_dir.path().join("shelf").join("dotfiles");
        assert!(target_dir.join(".testrc1").exists());
        assert!(target_dir.join(".testrc2").exists());

        // Verify original files still exist and contain the correct content
        assert!(test_file1.exists(), "Original file 1 should still exist");
        assert!(test_file2.exists(), "Original file 2 should still exist");

        let content1 = fs::read_to_string(&test_file1).await?;
        assert_eq!(
            content1, "test content 1",
            "Original file 1 content should be unchanged"
        );

        let content2 = fs::read_to_string(&test_file2).await?;
        assert_eq!(
            content2, "test content 2",
            "Original file 2 content should be unchanged"
        );

        // Verify symlink content matches original files
        let symlink1_content = fs::read_to_string(target_dir.join(".testrc1")).await?;
        assert_eq!(
            symlink1_content, "test content 1",
            "Symlink 1 content should match original file"
        );

        let symlink2_content = fs::read_to_string(target_dir.join(".testrc2")).await?;
        assert_eq!(
            symlink2_content, "test content 2",
            "Symlink 2 content should match original file"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_add_nonexistent_file() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let nonexistent_file = temp_dir.path().join("nonexistent");

        let result = index.add(&nonexistent_file.to_str().unwrap()).await;
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_nonexistent_dotfile() -> Result<()> {
        let (_, mut index) = setup_test_environment().await?;
        let result = index.remove_multi(&["nonexistent"]);

        assert_eq!(result.len(), 1, "Expected one result");
        assert!(
            result[0].is_err(),
            "Removing nonexistent dotfile should fail"
        );

        let error_message = result[0].as_ref().unwrap_err().to_string().to_lowercase();
        assert!(
            error_message.contains("not found") || error_message.contains("doesn't exist"),
            "Error message should indicate the dotfile was not found"
        );

        Ok(())
    }
    #[tokio::test]
    async fn test_file_conflict_handling() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let test_file = create_test_file(temp_dir.path(), ".testrc", "original content").await?;

        // Create a conflicting file in the target directory
        let target_dir = temp_dir.path().join("shelf").join("dotfiles");
        fs::create_dir_all(&target_dir).await?;
        let conflicting_file = target_dir.join(".testrc");
        fs::write(&conflicting_file, "conflicting content").await?;

        // Add the original file to the index
        index.add(test_file.to_str().unwrap()).await?;

        // Perform the copy operation
        index.copy().await?;

        // Verify that the file exists and contains the correct content
        let copied_file = target_dir.join(".testrc");
        assert!(copied_file.exists(), "Copied file should exist");
        assert!(
            !copied_file.is_symlink(),
            "Copied file should not be a symlink"
        );

        let copied_content = fs::read_to_string(&copied_file).await?;
        assert_eq!(
            copied_content, "original content",
            "Copied file should contain the original content, overwriting the conflicting content"
        );

        // Add the original file to the index
        index.add(test_file.to_str().unwrap()).await?;

        // Create a conflicting file in the target directory
        let target_dir = temp_dir.path().join("shelf").join("dotfiles");
        fs::create_dir_all(&target_dir).await?;
        let conflicting_file = target_dir.join(".testrc");
        fs::write(&conflicting_file, "conflicting content").await?;

        // Perform the copy operation
        index.copy().await?;

        // Verify that the file exists and contains the correct content
        let copied_file = target_dir.join(".testrc");
        assert!(copied_file.exists(), "Copied file should exist");
        assert!(
            !copied_file.is_symlink(),
            "Copied file should not be a symlink"
        );

        let copied_content = fs::read_to_string(&copied_file).await?;
        assert_eq!(
            copied_content, "original content",
            "Copied file should contain the original content, overwriting the conflicting content"
        );

        // Verify that the original file remains unchanged
        let original_content = fs::read_to_string(&test_file).await?;
        assert_eq!(
            original_content, "original content",
            "Original file content should remain unchanged"
        );

        // Add the dotfile to the index
        index.add(test_file.to_str().unwrap()).await?;

        // Create a conflicting file in the target directory
        let target_dir = temp_dir.path().join("shelf").join("dotfiles");
        fs::create_dir_all(&target_dir).await?;
        let conflicting_file = target_dir.join(".testrc");
        fs::write(&conflicting_file, "conflicting content").await?;

        // Copy dotfiles, which should overwrite the conflicting file
        index.copy().await?;

        // Verify that the file exists and contains the correct content
        let copied_file = target_dir.join(".testrc");
        assert!(copied_file.exists(), "Copied file should exist");
        assert!(
            !copied_file.is_symlink(),
            "Copied file should not be a symlink"
        );

        let copied_content = fs::read_to_string(&copied_file).await?;
        assert_eq!(
            copied_content, "original content",
            "Copied file should contain the original content, overwriting the conflicting content"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_update_existing_dotfile() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let initial_file = create_test_file(temp_dir.path(), ".testrc", "initial content").await?;
        index.add(initial_file.to_str().unwrap()).await?;

        let updated_file = create_test_file(temp_dir.path(), ".testrc", "updated content").await?;
        index.add(updated_file.to_str().unwrap()).await?;

        let dotfiles: Vec<_> = index.dotfiles.iter().collect();
        assert_eq!(dotfiles.len(), 1, "Should still have only one dotfile");

        let (_, entry) = dotfiles[0];
        let content = fs::read_to_string(&entry.source).await?;
        assert_eq!(content, "updated content", "Content should be updated");

        Ok(())
    }

    #[tokio::test]
    async fn test_link_nested_dotfiles() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let target_dir = index.target_directory.clone();

        // Create a deeply nested directory structure
        let nested_dir = temp_dir.path().join("level1").join("level2").join("level3");
        fs::create_dir_all(&nested_dir).await?;

        // Create files at different levels of nesting
        let root_file = create_test_file(temp_dir.path(), ".rootrc", "root content").await?;
        let level1_file = create_test_file(
            &temp_dir.path().join("level1"),
            ".level1rc",
            "level1 content",
        )
        .await?;
        let level3_file = create_test_file(&nested_dir, ".level3rc", "level3 content").await?;

        // Add all files to the index
        index.add(root_file.to_str().unwrap()).await?;
        index.add(level1_file.to_str().unwrap()).await?;
        index.add(level3_file.to_str().unwrap()).await?;

        // Perform the copy operation
        index.copy().await?;

        // Verify that all files are copied correctly
        assert!(
            target_dir.join(".rootrc").exists(),
            "Root file should exist"
        );
        assert!(
            target_dir.join(".level1rc").exists(),
            "Level 1 file should exist"
        );
        assert!(
            target_dir.join(".level3rc").exists(),
            "Level 3 file should exist"
        );

        // Create a new nested file
        let nested_file = create_test_file(&nested_dir, ".nestedrc", "nested content").await?;
        index.add(nested_file.to_str().unwrap()).await?;

        // Perform another copy operation
        index.copy().await?;

        // Verify that the new nested file is copied correctly
        let copied_nested_file = target_dir.join(".nestedrc");
        assert!(
            copied_nested_file.exists(),
            "Copied nested file should exist"
        );
        assert!(
            !copied_nested_file.is_symlink(),
            "Copied nested file should not be a symlink"
        );
        let content = fs::read_to_string(&copied_nested_file).await?;
        assert_eq!(
            content, "nested content",
            "Copied nested file content should match"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_broken_symlinks() -> Result<()> {
        let (temp_dir, mut index) = setup_test_environment().await?;
        let test_file = create_test_file(temp_dir.path(), ".testrc", "test content").await?;
        let target_dir = temp_dir.path().join("shelf").join("dotfiles");
        fs::create_dir_all(&target_dir).await?;

        // Create a broken symlink
        let broken_symlink = target_dir.join(".testrc");
        std::os::unix::fs::symlink("/nonexistent/path", &broken_symlink)?;

        // Add the original file to the index
        index.add(test_file.to_str().unwrap()).await?;

        // Perform the copy operation
        index.copy().await?;

        // Verify that the broken symlink has been replaced with the correct file
        assert!(broken_symlink.exists(), "Copied file should exist");
        assert!(
            !broken_symlink.is_symlink(),
            "Copied file should not be a symlink"
        );
        let content = fs::read_to_string(&broken_symlink).await?;
        assert_eq!(
            content, "test content",
            "Copied file should contain the correct content"
        );

        // Create an existing file (non-symlink) in the target directory
        let existing_file = target_dir.join(".existingrc");
        fs::write(&existing_file, "existing content").await?;

        // Add a new file to replace the existing one
        let new_file = create_test_file(temp_dir.path(), ".existingrc", "new content").await?;
        index.add(new_file.to_str().unwrap()).await?;

        // Perform another copy operation
        index.copy().await?;

        // Verify that the existing file has been replaced with the new content
        assert!(existing_file.exists(), "Existing file should still exist");
        assert!(
            !existing_file.is_symlink(),
            "Existing file should not be a symlink"
        );
        let content = fs::read_to_string(&existing_file).await?;
        assert_eq!(
            content, "new content",
            "Existing file should be updated with new content"
        );

        Ok(())
    }
}
