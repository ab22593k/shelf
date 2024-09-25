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
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::github::{RemoteHost, RemoteRepos};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dotfiles {
    pub dotfiles: HashMap<String, Index>,
    pub target_directory: PathBuf,
    pub config: RemoteHost,
}

/// Represents a single dotfile entry with its source path and index information.
///
/// This struct holds the information for an individual dotfile, including its
/// source path and metadata stored in the IndexEntry.
///
/// # Fields
///
/// * `source` - The source path of the dotfile.
/// * `inode` - The inode number of the dotfile.
/// * `version` - The version of the dotfile manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub source: PathBuf,
    pub inode: u64,
    version: u8,
}

impl Index {
    pub fn new(source: PathBuf, inode: u64, version: u8) -> Self {
        Self {
            source,
            inode,
            version,
        }
    }

    pub fn update_ino(&mut self, inode: u64) {
        self.inode = inode;
    }

    pub fn inc_version(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    pub fn get_source(&self) -> &PathBuf {
        &self.source
    }

    pub fn get_inode(&self) -> u64 {
        self.inode
    }

    pub fn get_version(&self) -> u8 {
        self.version
    }
}

/// A type alias for the iterator returned by `list_dotfiles`.
pub type DotfileIterator<'a> = std::collections::hash_map::Iter<'a, String, Index>;
async fn copy_dir_recursive(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    if !src.is_dir() {
        return Err(anyhow::anyhow!("Source is not a directory"));
    }

    if !dst.exists() {
        fs::create_dir_all(dst).await?;
    }

    let mut dir = fs::read_dir(src).await?;
    while let Some(entry) = dir.next_entry().await? {
        let entry_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(file_name);

        if entry_path.is_dir() {
            let recursive_copy = Box::pin(copy_dir_recursive(&entry_path, &dst_path));
            recursive_copy.await?;
        } else {
            fs::copy(&entry_path, &dst_path).await?;
        }
    }

    Ok(())
}

impl Dotfiles {
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
    pub async fn new(target_directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&target_directory)
            .await
            .context("Failed to create target directory")?;

        let shelf = Self {
            dotfiles: HashMap::new(),
            target_directory,
            config: RemoteHost::new(RemoteRepos::Github, String::new()),
        };

        Ok(shelf)
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
    pub async fn add<P: AsRef<Path>>(&mut self, file: P) -> Result<()> {
        let source = file.as_ref().to_path_buf();
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

        let metadata = fs::metadata(&source).await?;
        let index = Index {
            source: source.clone(),
            inode: metadata.ino(),
            version: 1,
        };

        self.dotfiles.insert(name.clone(), index);
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
    pub fn remove_multi(&mut self, fnames: &[&str]) -> Vec<Result<()>> {
        let mut results = Vec::new();
        for fname in fnames {
            let result = self.remove_single(fname);
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

        let target = self.target_directory.join(name);
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
            let target_path = self.target_directory.join(name);

            if !dotfile.source.exists() {
                eprintln!(
                    "{} Source file does not exist: {}",
                    "Error:".red().bold(),
                    dotfile.source.display()
                );
                continue;
            }

            // Create parent directory if it doesn't exist
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            // Handle existing files, directories, or symlinks
            if target_path.exists() || target_path.symlink_metadata().is_ok() {
                if target_path.is_symlink() {
                    fs::remove_file(&target_path).await?;
                } else if target_path.is_dir() {
                    fs::remove_dir_all(&target_path).await?;
                } else {
                    // Backup existing file
                    let backup_path = target_path.with_extension("bak");
                    fs::rename(&target_path, &backup_path).await?;
                    println!(
                        "{} Backed up existing file: {} -> {}",
                        "Info:".blue().bold(),
                        target_path.display(),
                        backup_path.display()
                    );
                }
            }

            // Copy the file or directory
            let copy_result = fs::copy(&dotfile.source, &target_path).await.map(|_| ());

            match copy_result {
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
