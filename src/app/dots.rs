use anyhow::Result;
use anyhow::anyhow;
use clap::{Args, Subcommand};
use colored::Colorize;
use git2::{Index, Repository, Statuses};
use std::path::{Path, PathBuf};
use std::{borrow::Cow, collections};
use tracing::debug;

use crate::git::verify_git_installation;
use crate::{error::Shelfor, utils::shine_success};

const HEADER_DIRECTORY: &str = "DIRECTORY";
const HEADER_ITEM: &str = "ITEM";
const HEADER_TYPE: &str = "TYPE";
const EMPTY_DIR_DOT: &str = ".";
const TYPE_DIR: &str = "Dir";
const TYPE_FILE: &str = "File";
const SAVE_SUCCESS: &str = "DotFs saved successfully";

#[derive(Args)]
pub struct DotsCMD {
    #[command(subcommand)]
    action: FileAction,
}

#[derive(Subcommand)]
pub enum FileAction {
    /// Track files for management.
    Track {
        /// Paths to the files to track.
        paths: Vec<PathBuf>,
    },
    /// Remove files from management.
    Untrack {
        /// Paths to the files to untrack.
        paths: Vec<PathBuf>,
    },
    /// List all currently tracked files.
    List {
        /// List only modified files.
        #[arg(short, long)]
        dirty: bool,
    },
    /// Save files for management.
    Save,
}

pub async fn run(args: DotsCMD, mut repo: Dots) -> Result<()> {
    match args.action {
        FileAction::Track { paths } => {
            repo.track(&paths)?;
            for path in paths {
                println!("Tracking {}", path.display().to_string().bright_green());
            }
        }
        FileAction::Untrack { paths } => {
            repo.untrack(&paths)?;
            for path in paths {
                println!("Untracking {}", path.display().to_string().bright_red());
            }
        }
        FileAction::List { dirty } => {
            if dirty {
                repo.set_filter(ListFilter::Modified);
            }
            let paths = repo.collect::<Vec<_>>();
            let paths_by_dir = group_tabs_by_directory(paths);
            print_grouped_paths(&paths_by_dir);
        }
        FileAction::Save => {
            repo.save_local_changes()?;
            println!("{}", SAVE_SUCCESS.bright_green());
        }
    }
    Ok(())
}

fn group_tabs_by_directory(paths: Vec<PathBuf>) -> collections::BTreeMap<PathBuf, Vec<PathBuf>> {
    debug!("Grouping {} paths by directory", paths.len());
    let mut paths_by_dir: collections::BTreeMap<PathBuf, Vec<PathBuf>> =
        collections::BTreeMap::new();
    for file in paths {
        let parent = file.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
        paths_by_dir.entry(parent).or_default().push(file);
    }

    paths_by_dir
}

fn get_home_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn display_path_relative_to_home<'a>(path: &'a Path, home: &'a Path) -> Cow<'a, Path> {
    path.strip_prefix(home)
        .map_or_else(|_| Cow::Borrowed(path), Cow::Borrowed)
}

fn print_grouped_paths(paths_by_dir: &collections::BTreeMap<PathBuf, Vec<PathBuf>>) {
    let home = get_home_dir();

    // Determine max widths for columns based on uncolored strings
    let mut max_dir_len = HEADER_DIRECTORY.len();
    let mut max_item_len = HEADER_ITEM.len();

    // Collect all rows as plain strings first to calculate accurate column widths
    let mut rows: Vec<(String, String, String)> = Vec::new();

    for (dir, files) in paths_by_dir.iter() {
        let display_dir_cow = display_path_relative_to_home(dir, &home);
        let display_dir_str = if display_dir_cow.as_os_str().is_empty() {
            EMPTY_DIR_DOT.to_string() // Represent empty path as '.' for display
        } else {
            display_dir_cow.display().to_string()
        };
        max_dir_len = max_dir_len.max(display_dir_str.len());

        for file in files {
            let item_name_cow = file.file_name().unwrap_or_default().to_string_lossy();
            let item_name_str = item_name_cow.to_string();
            max_item_len = max_item_len.max(item_name_str.len());

            let item_type = if file.is_dir() { TYPE_DIR } else { TYPE_FILE };
            rows.push((
                display_dir_str.clone(),
                item_name_str,
                item_type.to_string(),
            ));
        }
    }

    // Print Headers
    println!(
        "{:<width_dir$} {:<width_item$} {:<4}",
        HEADER_DIRECTORY.bold(),
        HEADER_ITEM.bold(),
        HEADER_TYPE.bold(),
        width_dir = max_dir_len,
        width_item = max_item_len,
    );
    println!(
        "{:-<width_dir$} {:-<width_item$} {:-<4}",
        "",
        "",
        "",
        width_dir = max_dir_len,
        width_item = max_item_len,
    );

    // Print Data Rows
    for (dir_str, item_name, item_type) in rows {
        // Pad the string first to the determined width, then apply color.
        // This ensures padding is based on visual length, not byte length including ANSI escape codes.
        let padded_dir = format!("{dir_str: <max_dir_len$}");
        let padded_item = format!("{item_name: <max_item_len$}");
        let padded_type = format!("{item_type: <4}");

        println!(
            "{} {} {}",
            padded_dir.blue().bold(),
            padded_item.bright_green(),
            padded_type.cyan(),
        );
    }
}

/// Manages system configuration files using a bare Git repository in the user's home directory.
pub struct Dots {
    bare: Repository,
    filter: ListFilter,
    filtered_entries: Vec<PathBuf>, // Pre-collected entries for iteration
    iter_index: usize,              // Tracks iteration progress
}

impl std::fmt::Debug for Dots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Git directory (bare repo path)
        let git_dir = self.bare.path().display();

        // Workdir may be absent for bare repos; present as string if available
        let workdir = self
            .bare
            .workdir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());

        // Human readable filter
        let filter = match self.filter {
            ListFilter::All => "All",
            ListFilter::Modified => "Modified",
        };

        // Attempt to get number of tracked entries from the index; fall back gracefully on error
        let tracked_entries = match self.bare.index() {
            Ok(idx) => idx.len().to_string(),
            Err(_) => "-".to_string(),
        };

        write!(
            f,
            "Dots {{ git_dir: {git_dir}, workdir: {workdir}, filter: {filter}, tracked_entries: {tracked_entries} }}"
        )
    }
}

impl Dots {
    pub fn new(git_dir: PathBuf, work_tree: PathBuf) -> Result<Self> {
        verify_git_installation()?;

        let repo = match Repository::open_bare(&git_dir) {
            Ok(repo) => repo,
            Err(_) => Repository::init_bare(&git_dir)?,
        };
        repo.set_workdir(&work_tree, false)?;

        Ok(Self {
            bare: repo,
            filter: ListFilter::All,
            filtered_entries: Vec::new(),
            iter_index: 0,
        })
    }

    /// Generic helper for operations that involve iterating over paths, validating, and modifying the index.
    fn apply_to_paths<F: Fn(&Dots, &Path, &mut Index) -> Result<()>>(
        &mut self,
        paths: &[PathBuf],
        action: F,
        success_message: &str,
    ) -> Result<()> {
        let mut index = self.get_index()?;

        for path in paths {
            self.validate_path(path)?;
            action(self, path, &mut index)?;
        }

        self.write_index(&mut index)?;
        shine_success(success_message);
        Ok(())
    }

    /// Tracks the specified paths by adding them to the Git index.
    pub fn track(&mut self, paths: &[PathBuf]) -> Result<()> {
        self.apply_to_paths(
            paths,
            |s, path, index| s.add_path(path, index),
            "Tabs tracked successfully",
        )
    }

    /// Untracks the specified paths by removing them from the Git index.
    pub fn untrack(&mut self, paths: &[PathBuf]) -> Result<()> {
        self.apply_to_paths(
            paths,
            |s, path, index| s.remove_path_or_dir(path, index),
            "Tabs untracked successfully",
        )
    }

    /// Commits staged changes with a default message.
    pub fn save_local_changes(&self) -> Result<String> {
        let mut index = self.get_index()?;
        let statuses = self.repository_status()?;

        if self.verify_staged_changes(&statuses).is_err() {
            return Err(anyhow!("No changes to commit"));
        }

        let signature = self.bare.signature()?;
        let commit_message = self.changes_recap(&statuses);
        let commit_tree = self.prepare_commit_tree(&mut index)?;
        let parent_commits = self.get_parent_commits()?;

        self.create_commit(&signature, &commit_message, &commit_tree, &parent_commits)?;

        Ok(commit_message)
    }

    /// Creates and returns a Git tree for the commit
    fn prepare_commit_tree(&self, index: &mut Index) -> Result<git2::Tree<'_>> {
        let tree_oid = index.write_tree()?;
        Ok(self.bare.find_tree(tree_oid)?)
    }

    /// Creates a new commit with the given components
    fn create_commit(
        &self,
        signature: &git2::Signature,
        message: &str,
        tree: &git2::Tree,
        parents: &[git2::Commit],
    ) -> Result<git2::Oid> {
        Ok(self.bare.commit(
            Some("HEAD"),
            signature,
            signature,
            message,
            tree,
            &parents.iter().collect::<Vec<_>>(),
        )?)
    }

    /// Sets the filter for listing files and resets the iterator.
    pub fn set_filter(&mut self, filter: ListFilter) {
        if self.filter != filter {
            self.filter = filter;
            self.reset_iterator();
        }
    }

    /// Converts a git2::IndexEntry path to a PathBuf relative to the workdir.
    fn index_entry_to_pathbuf(&self, entry: &git2::IndexEntry) -> Result<PathBuf, Shelfor> {
        let path_str = std::str::from_utf8(&entry.path).map_err(|_| Shelfor::InvalidUtf8Path)?;
        Ok(self.workdir()?.join(path_str))
    }

    /// Collects filtered entries for iteration based on the current filter.
    fn collect_filtered_entries(&mut self) -> Result<()> {
        let index = self.get_index()?;
        self.filtered_entries = index
            .iter()
            .filter_map(|entry| {
                self.index_entry_to_pathbuf(&entry).ok().and_then(|path| {
                    self.matches_filter(&path)
                        .ok()
                        .and_then(|m| m.then_some(path))
                })
            })
            .collect();
        Ok(())
    }

    /// Adds a path (file or directory) to the index.
    fn add_path(&self, path: &Path, index: &mut Index) -> Result<()> {
        if path.is_dir() {
            self.add_recursive(path, index)
        } else {
            self.add_one(path, index)
        }
    }

    /// Removes a path (file or directory) from the index.
    fn remove_path_or_dir(&self, path: &Path, index: &mut Index) -> Result<()> {
        let relative = self.get_relative_path(path)?;
        index.remove_all([relative], None)?;
        Ok(())
    }

    /// Checks if a path matches the current filter.
    fn matches_filter(&self, path: &Path) -> Result<bool> {
        match self.filter {
            ListFilter::All => {
                debug!("Filter: All - path {} matches", path.display());
                Ok(true)
            }
            ListFilter::Modified => {
                let relative = self.get_relative_path(path)?;
                let status = self.bare.status_file(relative)?;
                let is_modified = status.contains(git2::Status::WT_MODIFIED);
                debug!(
                    "Filter: Modified - path {} is modified: {}",
                    path.display(),
                    is_modified
                );
                Ok(is_modified)
            }
        }
    }

    /// Generates a summary of changes for the commit message.
    fn changes_recap(&self, statuses: &Statuses) -> String {
        let mut msg = String::from("Tracked tabs updated:\n");
        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                msg.push_str(&format!("  - {:?}: {}\n", entry.status(), path));
            }
        }
        msg
    }

    /// Validates a path before operations.
    fn validate_path(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(Shelfor::PathNotFound(path.to_path_buf()).into());
        }
        if !path.starts_with(self.workdir()?) {
            return Err(Shelfor::OutsideWorkTree(path.to_path_buf()).into());
        }
        Ok(())
    }

    /// Adds a single file to the index.
    fn add_one(&self, path: &Path, index: &mut Index) -> Result<()> {
        let relative = self.get_relative_path(path)?;
        index.add_path(relative)?;
        Ok(())
    }

    /// Recursively adds all files in a directory to the index.
    fn add_recursive(&self, path: &Path, index: &mut Index) -> Result<()> {
        let relative = self.get_relative_path(path)?;
        index.add_all([relative], git2::IndexAddOption::DEFAULT, None)?;
        Ok(())
    }

    /// Retrieves the parent commits for the current HEAD.
    fn get_parent_commits(&self) -> Result<Vec<git2::Commit<'_>>> {
        match self.bare.head() {
            Ok(head) => match head.target() {
                Some(oid) => match self.bare.find_commit(oid) {
                    Ok(commit) => Ok(vec![commit]),
                    Err(e) => Err(e.into()),
                },
                None => Ok(vec![]),
            },
            Err(_) => Ok(vec![]),
        }
    }

    /// Retrieves the repository index.
    fn get_index(&self) -> Result<Index> {
        Ok(self.bare.index().map_err(Shelfor::Git)?)
    }

    /// Writes the index to disk.
    fn write_index(&self, index: &mut Index) -> Result<()> {
        index.write().map_err(Shelfor::Git)?;
        Ok(())
    }

    /// Gets the working directory of the repository.
    fn workdir(&self) -> Result<&Path, Shelfor> {
        self.bare.workdir().ok_or(Shelfor::GitNotInstalled)
    }

    /// Computes the relative path from the working directory.
    fn get_relative_path<'a>(&self, path: &'a Path) -> Result<&'a Path, Shelfor> {
        Ok(path.strip_prefix(self.workdir()?)?)
    }

    /// Retrieves the repository status.
    fn repository_status(&self) -> Result<Statuses<'_>> {
        let mut opts = git2::StatusOptions::new();
        opts.include_ignored(false)
            .include_untracked(false)
            .include_unmodified(false)
            .show(git2::StatusShow::Index);
        Ok(self.bare.statuses(Some(&mut opts))?)
    }

    /// Checks if a Git status entry indicates a staged or modified change.
    fn is_staged_or_modified(&self, status: git2::Status) -> bool {
        status.contains(git2::Status::INDEX_NEW)
            || status.contains(git2::Status::INDEX_MODIFIED)
            || status.contains(git2::Status::INDEX_DELETED)
            || status.contains(git2::Status::WT_MODIFIED)
            || status.contains(git2::Status::WT_NEW)
    }

    /// Verifies that there are staged changes to commit.
    fn verify_staged_changes(&self, statuses: &Statuses) -> Result<()> {
        let has_changes = statuses
            .iter()
            .any(|entry| self.is_staged_or_modified(entry.status()));

        has_changes
            .then_some(())
            .ok_or_else(|| anyhow!("No staged changes to commit"))
    }

    /// Resets the iterator state.
    fn reset_iterator(&mut self) {
        self.iter_index = 0;
        self.filtered_entries.clear();
    }
}

impl Iterator for Dots {
    type Item = PathBuf;

    /// Returns the next file path matching the current filter, or `None` if exhausted.
    fn next(&mut self) -> Option<Self::Item> {
        if self.iter_index == 0 {
            let _ = self.collect_filtered_entries();
        }

        self.filtered_entries
            .get(self.iter_index)
            .cloned()
            .inspect(|_| {
                self.iter_index += 1;
            })
    }
}

/// Filtering criteria for repository listings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFilter {
    /// Show all tracked files without filtering.
    All,
    /// Only show files modified in the working tree.
    Modified,
}

#[cfg(test)]
mod tests {

    use crate::git::verify_git_installation;

    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_file as symlink;
    use std::{env, fs};
    use tempfile::tempdir;
    use tracing::debug;

    const SHELF_BARE_NAME: &str = ".shelf";

    struct TestEnv {
        manager: Dots,
        _temp: tempfile::TempDir,
    }

    impl TestEnv {
        pub fn new() -> Result<Self> {
            // Initialize logging if needed
            init_test_logging();

            // Create a temporary directory
            let temp_dir = tempdir()?;
            let work_tree = temp_dir.path();
            let git_dir = work_tree.join(SHELF_BARE_NAME);

            // Initialize a bare repository in the temp directory
            fs::create_dir_all(&git_dir)?;
            let repo = Repository::init_bare(&git_dir)?;
            repo.set_workdir(work_tree, false)?;

            let mut config = repo.config()?;
            config.set_str("user.name", "Test User")?;
            config.set_str("user.email", "test@example.com")?;

            // Create RepositoryManager with the isolated repository
            let manager = Dots {
                bare: repo,
                filter: ListFilter::All,
                filtered_entries: Vec::new(),
                iter_index: 0,
            };

            Ok(Self {
                _temp: temp_dir,
                manager,
            })
        }

        pub fn workdir(&self) -> &Path {
            self.manager.bare.workdir().expect("Valid workdir")
        }

        pub fn create_test_file(&self, path: &str) -> PathBuf {
            let full_path = self.workdir().join(path);
            fs::create_dir_all(full_path.parent().unwrap()).unwrap();
            fs::write(&full_path, "test content").unwrap();
            full_path
        }

        fn create_test_dir(&self, path: &str) -> PathBuf {
            debug!("Creating test directory: {}", path);
            let full_path = self.workdir().join(path);
            fs::create_dir_all(&full_path).unwrap();
            full_path
        }

        fn create_symlink(&self, target: &Path, link_name: &str) -> PathBuf {
            debug!("Creating symlink: {} -> {}", link_name, target.display());
            let link_path = self.workdir().join(link_name);
            symlink(target, &link_path).unwrap();
            link_path
        }

        fn tracked_paths(&mut self) -> Vec<PathBuf> {
            debug!("Getting tracked paths");
            // Reset index before getting paths to ensure fresh repository state
            self.manager.reset_iterator();

            let mut result = Vec::new();
            while let Some(path) = self.manager.next() {
                result.push(path);
            }

            // Reset again after collection
            self.manager.reset_iterator();
            result.sort(); // Sort paths for consistent comparison
            result
        }
    }

    fn init_test_logging() {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();
    }

    #[test]
    fn git_installation_detection() -> Result<()> {
        debug!("Testing git installation detection");
        let original_path = env::var_os("PATH");

        // SAFETY: Test environment should run without concurrent access
        unsafe { env::set_var("PATH", "") }
        let err = verify_git_installation().unwrap_err();
        assert!(matches!(err, Shelfor::GitNotInstalled));

        // SAFETY: Test environment should run without concurrent access
        // Restoring original PATH value
        unsafe { env::set_var("PATH", original_path.unwrap_or_default()) }
        assert!(verify_git_installation().is_ok());

        Ok(())
    }

    #[test]
    fn repository_initialization_creates_required_files() -> Result<()> {
        debug!("Testing repository initialization");
        let env = TestEnv::new()?;
        let repo_path = env.manager.bare.path();

        assert!(repo_path.exists(), "Repository directory missing");
        assert!(repo_path.join("HEAD").exists(), "HEAD file missing");
        assert!(repo_path.join("config").exists(), "Config file missing");
        assert!(
            repo_path.join("objects").exists(),
            "Objects directory missing"
        );

        Ok(())
    }

    #[test]
    fn tracking_untracking_single_file() -> Result<()> {
        debug!("Testing tracking/untracking single file");
        let mut env = TestEnv::new()?;

        let test_file = env.create_test_file("test.txt");

        // Clear out any existing index state
        if let Ok(mut index) = env.manager.bare.index() {
            index.clear()?;
        }

        // Track and verify
        env.manager.track(&[test_file.clone()])?;
        let tracked = env.tracked_paths();
        let expected = vec![test_file.clone()];
        assert_eq!(tracked, expected, "Tracked paths did not match expected");

        // Untrack and verify
        env.manager.untrack(&[test_file.clone()])?;
        let empty_paths = env.tracked_paths();
        assert!(
            empty_paths.is_empty(),
            "Paths should be empty after untracking"
        );
        assert!(test_file.exists(), "File should persist");

        Ok(())
    }

    #[test]
    fn tracking_invalid_paths_returns_proper_errors() -> Result<()> {
        debug!("Testing invalid path handling");
        let mut env = TestEnv::new()?;

        // Non-existent path
        let missing = env.workdir().join("ghost.txt");
        let err = env.manager.track(&[missing]).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<Shelfor>(),
            Some(Shelfor::PathNotFound(_))
        ));

        // External path
        let external = if cfg!(windows) {
            PathBuf::from("C:\\Windows\\System32\\drivers\\etc\\hosts")
        } else {
            PathBuf::from("/etc/passwd")
        };
        let err = env.manager.track(&[external]).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<Shelfor>(),
            Some(Shelfor::OutsideWorkTree(_))
        ));

        Ok(())
    }

    #[test]
    fn nested_directory_tracking_includes_all_files() -> Result<()> {
        debug!("Testing nested directory tracking");
        let mut env = TestEnv::new()?;

        // Configure git user for commits before any operations
        env.manager.bare.config()?.set_str("user.name", "test")?;
        env.manager
            .bare
            .config()?
            .set_str("user.email", "test@example.com")?;

        let dir = env.create_test_dir("nested/directory");

        let file1 = env.create_test_file("nested/directory/file1.txt");
        let file2 = env.create_test_file("nested/directory/.hidden");
        let file3 = env.create_test_file("nested/directory/.gitignore");

        // Track directory
        env.manager.track(&[dir])?;
        let tracked = env.tracked_paths();

        assert!(tracked.contains(&file1), "Normal file missing");
        assert!(tracked.contains(&file2), "Hidden file missing");
        assert!(tracked.contains(&file3), "Gitignore file missing");

        Ok(())
    }

    #[test]
    fn modified_files_filter_shows_changes() -> Result<()> {
        debug!("Testing modified files filter");
        let mut env = TestEnv::new()?;

        // Configure git user for commits before any operations
        env.manager.bare.config()?.set_str("user.name", "test")?;
        env.manager
            .bare
            .config()?
            .set_str("user.email", "test@example.com")?;

        let test_file = env.create_test_file("test.txt");

        // Configure git user for commits
        env.manager.bare.config()?.set_str("user.name", "test")?;
        env.manager
            .bare
            .config()?
            .set_str("user.email", "test@example.com")?;

        // Initial tracking and commit
        env.manager.track(&[test_file.clone()])?;
        env.manager.save_local_changes()?;

        env.manager.set_filter(ListFilter::Modified);

        // Should show no changes initially
        assert!(env.tracked_paths().is_empty(), "Initial state");

        // Modify file
        fs::write(&test_file, "new content")?;

        let tracked = env.tracked_paths();

        assert_eq!(tracked.len(), 1, "After modification");
        assert_eq!(tracked[0], test_file);

        // Stage changes
        env.manager.track(&[test_file.clone()])?;
        env.manager.save_local_changes()?;

        assert!(
            env.tracked_paths().is_empty(),
            "Should be empty after committing"
        );

        Ok(())
    }
    #[test]
    fn special_files_handling() -> Result<()> {
        debug!("Testing special files handling");
        let mut env = TestEnv::new()?;
        let target = env.create_test_file("target.txt");

        env.manager.track(&[target.clone()])?;

        // Store tracked paths and sort for consistent comparison
        let mut tracked = env.tracked_paths();
        tracked.sort();
        let mut expected = vec![target];
        expected.sort();

        assert_eq!(tracked, expected, "Tracked paths did not match expected");

        Ok(())
    }

    #[test]
    #[cfg(unix)] // Skip on Windows since it doesn't support empty directory tracking
    fn empty_directory_tracking() -> Result<()> {
        debug!("Testing empty directory tracking");
        let mut env = TestEnv::new()?;
        let empty_dir = env.create_test_dir("empty");

        env.manager.track(&[empty_dir])?;
        assert!(env.tracked_paths().is_empty());

        Ok(())
    }

    #[test]
    fn filter_mode_selection() -> Result<()> {
        debug!("Testing filter mode selection");
        let mut env = TestEnv::new()?;

        // Configure git user for commits
        env.manager.bare.config()?.set_str("user.name", "test")?;
        env.manager
            .bare
            .config()?
            .set_str("user.email", "test@example.com")?;

        let test_file = env.create_test_file("test.txt");

        // Track file with All filter
        env.manager.set_filter(ListFilter::All);
        env.manager.track(&[test_file.clone()])?;

        // Verify All filter shows file
        let mut tracked = env.tracked_paths();
        assert!(!tracked.is_empty(), "All filter should show tracked files");
        tracked.clear();

        // Switch to Modified filter
        env.manager.set_filter(ListFilter::Modified);

        // Verify Modified filter shows no changes
        assert!(
            env.tracked_paths().is_empty(),
            "Modified filter should show no changes for unmodified files"
        );

        Ok(())
    }

    #[test]
    #[cfg(unix)] // Only run on Unix systems
    fn symlink_tracking() -> Result<()> {
        debug!("Testing symlink tracking");
        let mut env = TestEnv::new()?;

        // Configure git user for commits
        env.manager.bare.config()?.set_str("user.name", "test")?;
        env.manager
            .bare
            .config()?
            .set_str("user.email", "test@example.com")?;

        let target = env.create_test_file("target.txt");
        let link = env.create_symlink(&target, "link.txt");

        // Track only the symlink
        env.manager.track(&[link.clone()])?;

        // Get tracked paths to verify only symlink was tracked
        let tracked = env.tracked_paths();

        assert!(tracked.contains(&link), "Symlink should be tracked");
        assert!(
            !tracked.contains(&target),
            "Target file should not be tracked"
        );

        // Verify symlink points to right target
        let metadata = fs::symlink_metadata(&link)?;
        assert!(metadata.file_type().is_symlink(), "Should be symlink");

        // Untrack symlink
        env.manager.untrack(&[link.clone()])?;
        assert!(env.tracked_paths().is_empty(), "No paths should be tracked");

        // Original files should still exist
        assert!(target.exists(), "Target file should exist");
        assert!(link.exists(), "Symlink should exist");

        Ok(())
    }

    #[test]
    fn tracking_multiple_files_and_directories() -> Result<()> {
        let mut env = TestEnv::new()?;
        let file1 = env.create_test_file("file1.txt");
        let file2 = env.create_test_file("file2.txt");
        let dir = env.create_test_dir("dir");
        let file_in_dir = env.create_test_file("dir/file3.txt");

        env.manager
            .track(&[file1.clone(), file2.clone(), dir.clone()])?;
        let tracked = env.tracked_paths();
        assert!(tracked.contains(&file1));
        assert!(tracked.contains(&file2));
        assert!(tracked.contains(&file_in_dir));

        Ok(())
    }

    #[test]
    fn untracking_non_tracked_file() -> Result<()> {
        let mut env = TestEnv::new()?;
        let file = env.create_test_file("file.txt");

        // Attempt to untrack a file that isn't tracked
        let result = env.manager.untrack(&[file.clone()]);
        assert!(
            result.is_ok(),
            "Untracking non-tracked file should not fail"
        );

        // Verify that the file still exists and nothing was changed
        assert!(file.exists());
        assert!(env.tracked_paths().is_empty());

        Ok(())
    }

    #[test]
    fn commit_with_no_staged_changes() -> Result<()> {
        let env = TestEnv::new()?;

        // Attempt to commit without staging any changes
        let result = env.manager.save_local_changes();
        assert!(result.is_err(), "Should error with no changes");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("No changes to commit"),
            "Wrong error message"
        );

        Ok(())
    }

    #[test]
    fn iterator_behavior_with_filters() -> Result<()> {
        let mut env = TestEnv::new()?;
        let file1 = env.create_test_file("file1.txt");
        let file2 = env.create_test_file("file2.txt");

        env.manager.track(&[file1.clone(), file2.clone()])?;

        // Set filter to All
        env.manager.set_filter(ListFilter::All);
        let tracked_all: Vec<_> = env.tracked_paths();
        assert_eq!(tracked_all.len(), 2);
        assert!(tracked_all.contains(&file1));
        assert!(tracked_all.contains(&file2));

        // Modify file1
        fs::write(&file1, "modified content")?;

        // Set filter to Modified
        env.manager.set_filter(ListFilter::Modified);
        let tracked_modified: Vec<_> = env.tracked_paths();
        assert_eq!(tracked_modified.len(), 1);
        assert!(tracked_modified.contains(&file1));

        Ok(())
    }
}
