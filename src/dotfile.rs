//! This module provides functionality for managing dotfiles.
//!
//! It includes structures and methods for handling dotfile entries,
//! syncing dotfiles, and maintaining an index of dotfiles with their
//! metadata such as timestamps, file sizes, and hashes.

use anyhow::{Context, Result};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use tokio::fs;

/// Represents a collection of dotfiles with their metadata and operations.
///
/// This struct is the main container for managing dotfiles. It holds a map of
/// dotfile entries, the target directory for symlinks, the last update time,
/// and the version of the dotfile manager.
///
/// # Fields
///
/// * `dotfiles` - A HashMap containing DotfileEntry instances, keyed by their names.
/// * `target_directory` - The directory where symlinks to the dotfiles will be created.
/// * `last_update` - The timestamp of the last update to the dotfiles collection.
/// * `version` - The version string of the dotfile manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dotfiles {
    pub dotfiles: HashMap<String, DotfileEntry>,
    pub target_directory: PathBuf,
    pub last_update: SystemTime,
    pub version: String,
}

/// Represents a single dotfile entry with its source path and index information.
///
/// This struct holds the information for an individual dotfile, including its
/// source path and metadata stored in the IndexEntry.
///
/// # Fields
///
/// * `source` - The source path of the dotfile.
/// * `index` - An IndexEntry containing metadata about the dotfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotfileEntry {
    pub source: PathBuf,
    pub index: IndexEntry,
}

/// Stores metadata about a dotfile for tracking changes.
///
/// This struct contains information used to determine if a dotfile has been
/// modified since the last sync operation.
///
/// # Fields
///
/// * `timestamp` - The last modification time of the dotfile.
/// * `file_size` - The size of the dotfile in bytes.
/// * `hash` - A SHA-256 hash of the dotfile's contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub timestamp: SystemTime,
    pub file_size: u64,
    pub hash: String,
}

/// A type alias for the iterator returned by `list_dotfiles`.
pub type DotfileIterator<'a> = std::collections::hash_map::Iter<'a, String, DotfileEntry>;

impl Dotfiles {
    /// Checks if a dotfile needs to be updated based on its metadata.
    ///
    /// This method compares the current timestamp and file size of the dotfile
    /// with the values stored in its index entry. If either has changed, it
    /// indicates that the dotfile needs to be updated.
    ///
    /// # Arguments
    ///
    /// * `dotfile` - A reference to the DotfileEntry to check for updates.
    ///
    /// # Returns
    ///
    /// * `Result<bool>` - Ok(true) if the dotfile needs updating, Ok(false) otherwise.
    async fn needs_update(dotfile: &DotfileEntry) -> Result<bool> {
        let metadata = fs::metadata(&dotfile.source).await?;
        let current_timestamp = metadata.modified()?;
        let current_size = metadata.len();

        Ok(current_timestamp != dotfile.index.timestamp || current_size != dotfile.index.file_size)
    }

    /// Creates a symlink for a dotfile.
    ///
    /// This method creates a symlink at the specified target path, pointing to the source path.
    /// If a file or directory already exists at the target path, it will be removed before
    /// creating the symlink.
    ///
    /// # Arguments
    ///
    /// * `source` - The path to the original dotfile.
    /// * `target` - The path where the symlink should be created.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok(()) if the symlink was created successfully, or an error if it failed.
    async fn create_symlink(source: &Path, target: &Path) -> Result<()> {
        if let Ok(metadata) = fs::metadata(target).await {
            if metadata.is_dir() {
                fs::remove_dir_all(target)
                    .await
                    .context("Failed to remove existing directory")?;
            } else {
                fs::remove_file(target)
                    .await
                    .context("Failed to remove existing file")?;
            }
        }

        #[cfg(unix)]
        tokio::fs::symlink(source, target)
            .await
            .context("Failed to create symlink")?;

        #[cfg(windows)]
        tokio::fs::symlink_file(source, target)
            .await
            .context("Failed to create symlink")?;

        Ok(())
    }

    /// Updates the index entry for a dotfile.
    ///
    /// This method updates the timestamp, file size, and hash of the dotfile in its index entry.
    /// It reads the file contents and calculates a new SHA-256 hash.
    ///
    /// # Arguments
    ///
    /// * `dotfile` - A mutable reference to the DotfileEntry to update.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok(()) if the update was successful, or an error if it failed.
    async fn update_index_entry(dotfile: &mut DotfileEntry) -> Result<()> {
        let metadata = fs::metadata(&dotfile.source).await?;
        dotfile.index.timestamp = metadata.modified()?;
        dotfile.index.file_size = metadata.len();

        // Implement file hashing using SHA-256
        let mut file = fs::File::open(&dotfile.source).await?;
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192]; // 8KB buffer for efficient reading

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        dotfile.index.hash = format!("{:x}", hash);

        Ok(())
    }

    /// Creates a new instance of Dotfiles.
    ///
    /// This method initializes a new Dotfiles struct with an empty HashMap for dotfiles,
    /// sets the target directory for symlinks, initializes the last update time to now,
    /// and sets the version from the package version.
    ///
    /// # Arguments
    ///
    /// * `target_directory` - A path-like object representing the directory where symlinks will be created.
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - Ok(Dotfiles) if initialization was successful, or an error if it failed.
    pub async fn new<P: AsRef<Path>>(target_directory: P) -> Result<Self> {
        let target_directory = target_directory.as_ref().to_path_buf();
        fs::create_dir_all(&target_directory)
            .await
            .context("Failed to create target directory")?;

        Ok(Self {
            dotfiles: HashMap::new(),
            target_directory,
            last_update: SystemTime::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    /// Adds a new dotfile to the collection.
    ///
    /// This method adds a new dotfile to the Dotfiles collection. It performs the following steps:
    /// 1. Validates the source file exists and is not already in the collection.
    /// 2. Canonicalizes the source path.
    /// 3. Creates a new DotfileEntry with initial metadata.
    /// 4. Updates the index entry with current file information.
    /// 5. Adds the new entry to the dotfiles HashMap.
    ///
    /// # Arguments
    ///
    /// * `source` - A path-like object representing the dotfile to be added.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok(()) if the dotfile was added successfully, or an error if it failed.
    pub async fn add_dotfile<P: AsRef<Path>>(&mut self, source: P) -> Result<()> {
        let source = source.as_ref().to_path_buf();
        let name = source
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?
            .to_string();

        if self.dotfiles.contains_key(&name) {
            return Err(anyhow::anyhow!("Dotfile already exists: {}", name));
        }
        if !source.exists() {
            return Err(anyhow::anyhow!(
                "Source file does not exist: {}",
                source.display()
            ));
        }
        let source = fs::canonicalize(&source)
            .await
            .context("Failed to canonicalize source path")?;

        let mut dotfile_entry = DotfileEntry {
            source,
            index: IndexEntry {
                timestamp: SystemTime::now(),
                file_size: 0,
                hash: String::new(),
            },
        };

        Self::update_index_entry(&mut dotfile_entry).await?;

        self.dotfiles.insert(name, dotfile_entry);
        Ok(())
    }

    /// Removes a dotfile from the collection.
    ///
    /// This method removes a dotfile from the Dotfiles collection and deletes its symlink
    /// if it exists in the target directory. It performs the following steps:
    /// 1. Removes the dotfile entry from the dotfiles HashMap.
    /// 2. Attempts to remove the symlink from the target directory if it exists.
    /// 3. Updates the last_update timestamp.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the dotfile to be removed.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok(()) if the dotfile was removed successfully, or an error if it failed.
    pub fn remove_dotfile(&mut self, name: &str) -> Result<()> {
        self.dotfiles
            .remove(name)
            .ok_or_else(|| anyhow::anyhow!("Dotfile not found: {}", name))?;

        let target = self.target_directory.join(name);
        if target.exists() {
            std::fs::remove_file(&target)
                .context(format!("Failed to remove symlink for dotfile: {}", name))?;
        }
        self.last_update = SystemTime::now();
        Ok(())
    }

    /// Returns an iterator over all dotfiles in the collection.
    ///
    /// This method provides a way to iterate over all the dotfiles stored in the
    /// Dotfiles struct. It returns an iterator that yields tuples containing
    /// references to the dotfile name (as a String) and its corresponding DotfileEntry.
    ///
    /// # Returns
    ///
    /// * An iterator over (&String, &DotfileEntry) pairs.
    pub fn list_dotfiles(&self) -> DotfileIterator<'_> {
        self.dotfiles.iter()
    }

    /// Synchronizes all dotfiles in the collection.
    ///
    /// This method iterates through all dotfiles in the collection and performs the following steps:
    /// 1. Checks if the symlink exists in the target directory.
    /// 2. If the symlink doesn't exist or the dotfile needs updating, creates or updates the symlink.
    /// 3. Updates the index entry for the dotfile.
    /// 4. Prints status messages for each dotfile (synced or skipped).
    /// 5. Updates the last_update timestamp of the Dotfiles collection.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Ok(()) if all dotfiles were synced successfully, or an error if any operation failed.
    pub async fn sync_dotfiles(&mut self) -> Result<()> {
        for (name, dotfile) in self.dotfiles.iter_mut() {
            let target = self.target_directory.join(name);

            if !target.exists() || Self::needs_update(dotfile).await? {
                Self::create_symlink(&dotfile.source, &target).await?;
                println!(
                    "Synced: {} -> {}",
                    dotfile.source.display(),
                    target.display()
                );
                Self::update_index_entry(dotfile).await?;
            } else {
                println!("Skipped (up-to-date): {}", name);
            }
        }
        self.last_update = SystemTime::now();
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
    pub async fn add_multiple_dotfiles<P, I>(
        &mut self,
        sources: I,
    ) -> Vec<Result<(), anyhow::Error>>
    where
        P: AsRef<Path>,
        I: IntoIterator<Item = P>,
    {
        let mut results = Vec::new();
        for source in sources {
            results.push(self.add_dotfile(source).await);
        }
        results
    }
}
