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
    pub config_dir: PathBuf,
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

impl Dotfiles {
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

    pub async fn new(config_dir: PathBuf, target_directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&target_directory)
            .await
            .context("Failed to create target directory")?;

        Ok(Self {
            dotfiles: HashMap::new(),
            target_directory,
            config_dir,
            config: RemoteHost::new(RemoteRepos::Github, String::new())?,
        })
    }

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

        self.save(&self.config_dir).await?;

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
    // pub async fn add_multi<P, I>(&mut self, sources: I) -> Vec<Result<(), anyhow::Error>>
    // where
    //     P: AsRef<Path>,
    //     I: IntoIterator<Item = P>,
    // {
    //     println!("{}", "Adding multiple dotfiles...".blue().bold());
    //     let mut results = Vec::new();
    //     for source in sources {
    //         let result = self.add(source).await;
    //         if let Err(ref e) = result {
    //             println!("{} {}", "Failed to add dotfile:".red().bold(), e);
    //         }
    //         results.push(result);
    //     }
    //     println!("{}", "Finished adding dotfiles".green().bold());
    //     self.save(&self.config_dir).await?;
    //     results
    // }

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

    pub async fn save(&self, index_path: &PathBuf) -> Result<()> {
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
