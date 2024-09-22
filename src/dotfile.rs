//! This module provides functionality for managing dotfiles.
//!
//! It includes structures and methods for handling dotfile entries,
//! syncing dotfiles, and maintaining an index of dotfiles with their
//! metadata such as timestamps, file sizes, and hashes.

use anyhow::{anyhow, Context, Result};
use colored::*;
use serde::{Deserialize, Serialize};
use tokio::fs;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
    pub outdire: PathBuf,
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
/// * `inode` - The inode number of the dotfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub timestamp: SystemTime,
    pub inode: u64,
}

/// A type alias for the iterator returned by `list_dotfiles`.
pub type DotfileIterator<'a> = std::collections::hash_map::Iter<'a, String, DotfileEntry>;

impl Dotfiles {
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

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            dotfile.index.inode = metadata.ino();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            dotfile.index.inode = metadata.file_index().unwrap_or(0);
        }

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
            outdire: target_directory,
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
    pub async fn add<P: AsRef<Path>>(&mut self, source: P) -> Result<()> {
        let source = source.as_ref().to_path_buf();
        let name = source
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .ok_or_else(|| anyhow::anyhow!("{} Invalid file name", "Error:".red().bold()))?
            .to_string();

        if !source.exists() {
            return Err(anyhow::anyhow!(
                "{} Source file does not exist: {}",
                "Error:".red().bold(),
                source.display().to_string().cyan()
            ));
        }
        let source = fs::canonicalize(&source).await.context(format!(
            "{} Failed to canonicalize source path: {}",
            "Error:".red().bold(),
            source.display().to_string().cyan()
        ))?;

        let mut dotfile_entry = DotfileEntry {
            source: source.clone(),
            index: IndexEntry {
                timestamp: SystemTime::now(),
                inode: 0,
            },
        };

        Self::update_index_entry(&mut dotfile_entry).await?;

        self.dotfiles.insert(name.clone(), dotfile_entry);
        println!(
            "{} Added dotfile: {}",
            "Success:".green().bold(),
            name.cyan()
        );
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
    /// * `Vec<Result<()>>` - A vector of results, one for each dotfile attempted to be removed.
    pub fn remove_multi(&mut self, names: &[&str]) -> Vec<Result<()>> {
        let mut results = Vec::new();
        for name in names {
            let result = self.remove_single(name);
            results.push(result);
        }
        results
    }

    fn remove_single(&mut self, name: &str) -> Result<()> {
        self.dotfiles.remove(name).ok_or_else(|| {
            anyhow!(
                "{} Dotfile '{}' not found in the collection",
                "Error:".red().bold(),
                name.cyan()
            )
        })?;

        let target = self.outdire.join(name);
        if target.exists() {
            std::fs::remove_file(&target).context(format!(
                "{} Failed to remove file for dotfile: {}",
                "Error:".red().bold(),
                name.cyan()
            ))?;
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

    /// Returns an iterator over all dotfiles in the collection.
    ///
    /// This method provides a way to iterate over all the dotfiles stored in the
    /// Dotfiles struct. It returns an iterator that yields tuples containing
    /// references to the dotfile name (as a String) and its corresponding DotfileEntry.
    ///
    /// # Returns
    ///
    /// * A reference to the HashMap of dotfiles.
    pub fn list(&self) -> &HashMap<String, DotfileEntry> {
        &self.dotfiles
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
    pub async fn copy(&self) -> Result<()> {
        println!(
            "{}",
            "Copying dotfiles to the output directory...".blue().bold()
        );

        let mut success_count = 0;
        let total_count = self.dotfiles.len();

        for (name, dotfile) in &self.dotfiles {
            let target_path = self.outdire.join(name);

            if !dotfile.source.exists() {
                println!(
                    "{} Source file does not exist: {}",
                    "Warning:".yellow().bold(),
                    dotfile.source.display()
                );
                continue;
            }

            // Create parent directory if it doesn't exist
            if let Some(parent) = target_path.parent() {
                if let Err(e) = fs::create_dir_all(parent).await {
                    println!(
                        "{} Failed to create directory {}: {}",
                        "Error:".red().bold(),
                        parent.display(),
                        e
                    );
                    continue;
                }
            }

            // Copy the file
            match fs::copy(&dotfile.source, &target_path).await {
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
                    println!(
                        "{} Failed to copy {}: {}",
                        "Error:".red().bold(),
                        dotfile.source.display(),
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

    pub async fn save(&self, index_path: &PathBuf) -> Result<()> {
        let json = serde_json::to_string(&self)?;
        tokio::fs::write(index_path, json).await?;
        Ok(())
    }
}
