use std::{
    io,
    path::{self, Path, PathBuf},
};

use anyhow::{Result, anyhow};
use colored::Colorize;
use git2::{Index, Repository};
use thiserror::Error;
use walkdir::WalkDir;

use crate::utils::check_git_installation;

const SHELF_BARE_NAME: &str = ".shelf";

/// Manages system configuration files.
pub struct DotFs {
    bare: Repository,
    filter: ListFilter,
    iter_index: usize, // Tracks iteration progress
}

impl Default for DotFs {
    fn default() -> Self {
        check_git_installation().expect("Git must be installed");

        let user_dirs = directories::UserDirs::new()
            .ok_or(TabsError::HomeDirectoryNotFound)
            .unwrap();
        let work_tree = user_dirs.home_dir().canonicalize().unwrap();
        let git_dir = work_tree.join(SHELF_BARE_NAME);

        // First try to open existing repo, otherwise initialize a new one
        let repo = match Repository::open_bare(&git_dir) {
            Ok(repo) => repo,
            Err(_) => Repository::init_bare(&git_dir).unwrap(),
        };

        // Set the working directory for the repository
        repo.set_workdir(&work_tree, false).unwrap();

        Self {
            bare: repo,
            filter: ListFilter::All,
            iter_index: 0,
        }
    }
}

#[derive(Error, Debug)]
pub enum TabsError {
    #[error("Home directory not found")]
    HomeDirectoryNotFound,
    #[error("Path not found: {0:?}")]
    PathNotFound(PathBuf),
    #[error("Path is outside work tree: {0:?}")]
    OutsideWorkTree(PathBuf),
    #[error("Invalid UTF-8 in path")]
    InvalidUtf8Path,
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Path strip error: {0}")]
    StripPrefix(#[from] path::StripPrefixError),
    #[error("Git executable is not installed")]
    GitNotInstalled,
}

impl Iterator for DotFs {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        let index = match self.bare.index() {
            Ok(idx) => idx,
            Err(_) => return None,
        };

        let entries: Vec<_> = index.iter().collect();

        while self.iter_index < entries.len() {
            let entry = &entries[self.iter_index];
            self.iter_index += 1;

            match self.process_index_entry(entry, self.filter) {
                Ok(Some(path)) => return Some(path),
                Ok(None) => continue,
                Err(_) => continue,
            }
        }

        // Reset index when reaching end
        self.iter_index = 0;
        None
    }
}

/// Filtering criteria for repository listings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFilter {
    /// Show all `Tab`s without filtering
    All,
    /// Only show modified files in working tree
    Modified,
}

impl DotFs {
    pub fn track(&mut self, paths: &[PathBuf]) -> Result<()> {
        // Get repository index once at start
        let mut index = self.bare.index()?;

        // Process all paths and collect any errors
        for path in paths {
            // Validate path before attempting operations
            self.validate_path(path)?;

            // Use appropriate add method based on path type
            match path.is_dir() {
                true => self.add_recursive(path, &mut index)?,
                false => self.add_one(path, &mut index)?,
            }
        }

        // Write index changes back to disk
        index.write()?;
        print_success("Tabs tracked successfully");
        Ok(())
    }

    pub fn untrack(&mut self, paths: &[PathBuf]) -> Result<()> {
        let mut index = self.bare.index()?;
        for path in paths {
            self.validate_path(path)?;
            if path.is_dir() {
                WalkDir::new(path)
                    .into_iter()
                    .filter_map(Result::ok)
                    .filter(|e| e.file_type().is_file())
                    .try_for_each(|e| -> Result<()> {
                        self.remove_path(e.path(), &mut index)?;
                        Ok(())
                    })?;
            } else {
                self.remove_path(path, &mut index)?;
            }
        }
        index.write()?;
        print_success("Tabs untracked successfully");
        Ok(())
    }

    pub fn matches_filter(&self, path: &Path, filter: ListFilter) -> Result<bool> {
        match filter {
            ListFilter::All => Ok(true),
            ListFilter::Modified => {
                let relative_path = path.strip_prefix(self.bare.workdir().unwrap())?;
                let mut status_opts = git2::StatusOptions::new();
                status_opts.include_untracked(true);
                let status = self.bare.status_file(relative_path)?;
                Ok(status.contains(git2::Status::WT_MODIFIED))
            }
        }
    }

    /// Commits modified files in the repository with a generated message.
    pub fn pin_changes(&self) -> Result<String> {
        let mut index = self.bare.index().map_err(TabsError::Git)?;

        let statuses = self.repository_status()?;

        // Verify we have staged files
        let staged_files = statuses.iter().any(|entry| {
            entry.status().is_index_new()
                || entry.status().is_index_modified()
                || entry.status().is_index_deleted()
        });
        if !staged_files {
            return Err(anyhow!("No staged changes to commit"));
        }

        let tree_oid = index.write_tree()?;
        let tree = self.bare.find_tree(tree_oid)?;
        let signature = self.bare.signature()?;

        let commit_message = self.changes_recap(&statuses);

        let parents = match self.bare.head() {
            Ok(_) => {
                let commit = self.get_head_commit()?;
                vec![commit]
            }
            Err(_) => {
                vec![] /* No initial commit yet*/
            }
        };

        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        self.bare.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &commit_message,
            &tree,
            &parent_refs,
        )?;

        Ok(commit_message)
    }

    /// Create a summary of changes for the commit message.
    fn changes_recap(&self, statuses: &git2::Statuses) -> String {
        let mut msg = String::from("Tracked tabs updated:\n");
        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                msg.push_str(&format!("  - {:?}: {}\n", entry.status(), path));
            }
        }
        msg
    }

    /// Gets the HEAD commit of the repository.
    fn get_head_commit(&self) -> Result<git2::Commit> {
        let head = self.bare.head()?;
        let oid = head.target().ok_or_else(|| anyhow!("Detached HEAD"))?;
        Ok(self.bare.find_commit(oid)?)
    }

    /// Retrieves status information for all repository files
    fn repository_status(&self) -> Result<git2::Statuses> {
        let mut status_options = git2::StatusOptions::new();
        Ok(self
            .bare
            .statuses(Some(&mut status_options))
            .map_err(TabsError::Git)?)
    }

    fn process_index_entry(
        &self,
        entry: &git2::IndexEntry,
        filter: ListFilter,
    ) -> Result<Option<PathBuf>> {
        let path_str = std::str::from_utf8(&entry.path).map_err(|_| TabsError::InvalidUtf8Path)?;
        let workdir = self.bare.workdir().ok_or(TabsError::GitNotInstalled)?;
        let path = workdir.join(path_str);

        self.matches_filter(&path, filter)
            .map(|matches| matches.then_some(path))
    }

    fn validate_path(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(TabsError::PathNotFound(path.to_path_buf()).into());
        }

        let workdir = self.bare.workdir().ok_or(TabsError::GitNotInstalled)?;
        if !path.starts_with(workdir) {
            return Err(TabsError::OutsideWorkTree(path.to_path_buf()).into());
        }

        Ok(())
    }

    fn add_one(&self, path: &Path, index: &mut Index) -> Result<()> {
        let workdir = self.bare.workdir().ok_or(TabsError::GitNotInstalled)?;
        path.strip_prefix(workdir)
            .map_err(TabsError::StripPrefix)
            .and_then(|relative| index.add_path(relative).map_err(TabsError::Git))?;
        Ok(())
    }

    fn add_recursive(&self, path: &Path, index: &mut Index) -> Result<()> {
        WalkDir::new(path)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .try_for_each(|e| self.add_one(e.path(), index))
    }

    fn remove_path(&self, path: &Path, index: &mut Index) -> Result<bool> {
        let workdir = self.bare.workdir().ok_or(TabsError::GitNotInstalled)?;
        Ok(path
            .strip_prefix(workdir)
            .map_err(TabsError::StripPrefix)?
            .to_str()
            .ok_or(TabsError::InvalidUtf8Path)
            .map(|relative| index.remove_path(Path::new(relative)).is_ok())?)
    }

    // Helper method to reset iterator state
    fn reset_iterator(&mut self) {
        self.iter_index = 0;
    }

    pub fn set_filter(&mut self, filter: ListFilter) {
        if self.filter != filter {
            self.filter = filter;
            self.reset_iterator();
        }
    }
}

/// Success message with consistent styling formats
fn print_success(message: &str) {
    println!("{} {}", "✓".bright_green(), message.bold().green());
}

/// Verifies existence of staged changes
#[allow(dead_code)]
fn verify_staged_changes(index: &Index) -> Result<()> {
    let has_changes = index.iter().any(|entry| {
        let status = git2::Status::from_bits_truncate(entry.ino);
        status.is_index_new() || status.is_index_modified() || status.is_index_deleted()
    });

    has_changes
        .then_some(())
        .ok_or_else(|| anyhow!("No changes to commit"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_file as symlink;
    use std::sync::Mutex;
    use std::{env, fs};
    use tempfile::tempdir;
    use tracing::debug;

    // Global lock for environment-sensitive tests
    static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    //-------------------------
    // Test Helpers
    //-------------------------

    struct TestEnv {
        _temp_dir: tempfile::TempDir,
        manager: DotFs,
    }

    impl TestEnv {
        fn new() -> Result<Self> {
            init_test_logging();

            let _lock = ENV_LOCK.lock().unwrap(); // Acquire lock first

            let temp_dir = tempdir()?;
            unsafe { env::set_var("HOME", temp_dir.path()) };

            Ok(Self {
                _temp_dir: temp_dir,
                manager: DotFs::default(),
            })
        }

        fn workdir(&self) -> &Path {
            self.manager.bare.workdir().expect("Valid workdir")
        }

        fn create_test_file(&self, path: &str) -> PathBuf {
            debug!("Creating test file: {}", path);
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
            let mut result = Vec::new();
            while let Some(path) = self.manager.next() {
                result.push(path);
            }
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

    impl TabsError {
        pub fn is_git_not_installed(&self) -> bool {
            matches!(self, Self::GitNotInstalled)
        }

        pub fn is_path_not_found(&self) -> bool {
            matches!(self, Self::PathNotFound(_))
        }

        pub fn is_outside_work_tree(&self) -> bool {
            matches!(self, Self::OutsideWorkTree(_))
        }
    }

    //-------------------------
    // Test Cases
    //-------------------------

    #[test]
    fn git_installation_detection() -> Result<()> {
        debug!("Testing git installation detection");
        let _lock = ENV_LOCK.lock().unwrap();
        let original_path = env::var_os("PATH");

        // Test missing git
        // SAFETY: Test environment with mutex lock ensuring no concurrent access
        unsafe { env::set_var("PATH", "") }
        let err = check_git_installation().unwrap_err();
        let tabs_err = err.downcast_ref::<TabsError>().unwrap();
        assert!(tabs_err.is_git_not_installed());

        // SAFETY: Test environment with mutex lock ensuring no concurrent access
        // Restoring original PATH value
        unsafe { env::set_var("PATH", original_path.unwrap_or_default()) }
        assert!(check_git_installation().is_ok());

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
        let tabs_err = err.downcast_ref::<TabsError>().unwrap();
        assert!(tabs_err.is_path_not_found());

        // External path
        let external = if cfg!(windows) {
            PathBuf::from("C:\\Windows\\System32\\drivers\\etc\\hosts")
        } else {
            PathBuf::from("/etc/passwd")
        };
        let err = env.manager.track(&[external]).unwrap_err();
        let tabs_err = err.downcast_ref::<TabsError>().unwrap();
        assert!(tabs_err.is_outside_work_tree());

        Ok(())
    }

    #[test]
    fn nested_directory_tracking_includes_all_files() -> Result<()> {
        debug!("Testing nested directory tracking");
        let mut env = TestEnv::new()?;
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
        let test_file = env.create_test_file("test.txt");

        // Configure git user for commits
        env.manager.bare.config()?.set_str("user.name", "test")?;
        env.manager
            .bare
            .config()?
            .set_str("user.email", "test@example.com")?;

        // Initial tracking and commit
        env.manager.track(&[test_file.clone()])?;
        env.manager.pin_changes()?;

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
        env.manager.pin_changes()?;

        assert!(env.tracked_paths().is_empty(), "After staging");

        Ok(())
    }
    #[test]
    fn special_files_handling() -> Result<()> {
        debug!("Testing special files handling");
        let mut env = TestEnv::new()?;
        let target = env.create_test_file("target.txt");

        // Clear out any existing index state to avoid lock issues
        if let Ok(mut index) = env.manager.bare.index() {
            index.clear()?;
        }

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
    fn filter_mode_selection_works() -> Result<()> {
        debug!("Testing filter mode selection");
        let mut env = TestEnv::new()?;
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
}
