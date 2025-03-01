use std::{
    io,
    path::{self, Path, PathBuf},
};

use anyhow::{Result, anyhow};
use git2::{Index, Repository, Statuses};
use thiserror::Error;
use tracing::debug;

use crate::utils::{check_git_installation, print_success};

const SHELF_BARE_NAME: &str = ".shelf";

/// Initializes the DotFs repository in the user's home directory.
pub fn init_dotfs_repo() -> Result<Repository> {
    check_git_installation().expect("Git must be installed");

    let work_tree = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| DotFsError::HomeDirectoryNotFound)?
        .canonicalize()?;
    let git_dir = work_tree.join(SHELF_BARE_NAME);

    let repo = match Repository::open_bare(&git_dir) {
        Ok(repo) => repo,
        Err(_) => Repository::init_bare(&git_dir)?,
    };
    repo.set_workdir(&work_tree, false)?;
    Ok(repo)
}

#[derive(Error, Debug)]
pub enum DotFsError {
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

/// Manages system configuration files using a bare Git repository in the user's home directory.
pub struct DotFs {
    bare: Repository,
    filter: ListFilter,
    filtered_entries: Vec<PathBuf>, // Pre-collected entries for iteration
    iter_index: usize,              // Tracks iteration progress
}

impl Default for DotFs {
    fn default() -> Self {
        match init_dotfs_repo() {
            Ok(bare) => Self {
                bare,
                filter: ListFilter::All, // Default filter
                filtered_entries: Vec::new(),
                iter_index: 0,
            },
            Err(e) => panic!("Failed to initialize DotFs repository: {}", e),
        }
    }
}

impl DotFs {
    /// Tracks the specified paths by adding them to the Git index.
    pub fn track(&mut self, paths: &[PathBuf]) -> Result<()> {
        let mut index = self.get_index()?;

        for path in paths {
            self.validate_path(path)?;
            self.add_path(path, &mut index)?;
        }

        self.write_index(&mut index)?;
        print_success("Tabs tracked successfully");
        Ok(())
    }

    /// Untracks the specified paths by removing them from the Git index.
    pub fn untrack(&mut self, paths: &[PathBuf]) -> Result<()> {
        let mut index = self.get_index()?;

        for path in paths {
            self.validate_path(path)?;
            self.remove_path_or_dir(path, &mut index)?;
        }

        self.write_index(&mut index)?;
        print_success("Tabs untracked successfully");
        Ok(())
    }

    /// Commits staged changes with a default message.
    pub fn save_local_changes(&self) -> Result<String> {
        // Get current repository state
        let mut index = self.get_index()?;
        let statuses = self.repository_status()?;

        // Check for staged changes
        if self.verify_staged_changes(&statuses).is_err() {
            return Err(anyhow!("No changes to commit"));
        }

        // Prepare commit components
        let signature = self.bare.signature()?;
        let commit_message = self.changes_recap(&statuses);
        let commit_tree = self.prepare_commit_tree(&mut index)?;
        let parent_commits = self.get_parent_commits()?;

        // Create the commit
        self.create_commit(&signature, &commit_message, &commit_tree, &parent_commits)?;

        Ok(commit_message)
    }

    /// Creates and returns a Git tree for the commit
    fn prepare_commit_tree(&self, index: &mut Index) -> Result<git2::Tree> {
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

    /// Adds a Git remote reference to the repository.
    ///
    /// # Arguments
    ///
    /// * `remote_name` - The name of the remote to add (e.g. "origin")
    /// * `url` - The URL of the remote repository
    ///
    /// # Returns
    ///
    /// * `Ok(remote_name)` if the remote was added successfully
    /// * `Err(DotFsError::Git)` if there was an error creating the remote
    pub fn add_remote(&mut self, name: impl AsRef<str>, url: impl AsRef<str>) -> Result<String> {
        let remote_name = name.as_ref().to_string();
        let remote_url = url.as_ref();

        debug!("Adding remote: {} -> {}", remote_name, remote_url);

        let result = match self.bare.find_remote(&remote_name) {
            Ok(_) => {
                debug!(
                    "Remote {} already exists, updating URL to {}",
                    remote_name, remote_url
                );
                self.bare.remote_set_url(&remote_name, remote_url)
            }
            Err(_) => {
                debug!("Creating new remote: {} -> {}", remote_name, remote_url);
                match self.bare.remote(&remote_name, remote_url) {
                    Ok(remote) => {
                        debug!("Remote created with URL: {:?}", remote.url());
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
        };

        result?;
        Ok(remote_name)
    }
    /// Pushes the bare repository to a remote
    ///
    /// # Arguments
    ///
    /// * `remote` - Name of the remote to push to (e.g. "origin")
    /// * `branch` - Name of the branch to push (e.g. "master", "main")
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the push was successful
    /// * `Err(DotFsError::Git)` if there was an error during push
    pub fn push(&self, remote: impl AsRef<str>, branch: impl AsRef<str>) -> Result<()> {
        let remote_name = remote.as_ref();
        let branch_name = branch.as_ref();

        debug!("Pushing to remote {} branch {}", remote_name, branch_name);

        let mut remote = self.bare.find_remote(remote_name)?;
        let refspec = format!("refs/heads/{}", branch_name);

        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, allowed_types| {
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                // Try common SSH key locations
                let ssh_key_paths = if cfg!(windows) {
                    vec![
                        directories::UserDirs::new().map(|h| h.home_dir().join(".ssh").join("id_rsa")),
                        directories::UserDirs::new().map(|h| h.home_dir().join(".ssh").join("id_ed25519")),
                    ]
                } else {
                    vec![
                        Some(PathBuf::from("/home").join(username_from_url.unwrap_or("git")).join(".ssh/id_rsa")),
                        Some(PathBuf::from("/home").join(username_from_url.unwrap_or("git")).join(".ssh/id_ed25519")),
                    ]
                };

                for key_path in ssh_key_paths.into_iter().flatten() {
                    if key_path.exists() {
                        if let Ok(cred) = git2::Cred::ssh_key(
                            username_from_url.unwrap_or("git"),
                            None,
                            &key_path,
                            None,
                        ) {
                            return Ok(cred);
                        }
                    }
                }
                Err(git2::Error::from_str("No SSH keys found"))
            } else if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
                // Try fetching from environment variables
                if let (Ok(user), Ok(pass)) =
                    (std::env::var("GIT_USERNAME"), std::env::var("GIT_PASSWORD"))
                {
                    git2::Cred::userpass_plaintext(&user, &pass)
                } else if let (Ok(token), _) = (std::env::var("GITHUB_TOKEN"), ()) {
                    // Try GitHub token authentication
                    git2::Cred::userpass_plaintext("git", &token)
                } else {
                    Err(git2::Error::from_str("No Git credentials found in environment variables. Set GIT_USERNAME and GIT_PASSWORD, or GITHUB_TOKEN"))
                }
            } else {
                Err(git2::Error::from_str("No supported authentication methods found. Configure SSH keys or set Git credentials in environment variables"))
            }
        });

        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        remote.push(&[&refspec], Some(&mut push_opts))?;

        debug!("Successfully pushed to {}:{}", remote_name, branch_name);
        Ok(())
    }

    /// Sets the filter for listing files and resets the iterator.
    pub fn set_filter(&mut self, filter: ListFilter) {
        if self.filter != filter {
            self.filter = filter;
            self.reset_iterator();
        }
    }

    /// Collects filtered entries for iteration based on the current filter.
    fn collect_filtered_entries(&mut self) -> Result<()> {
        let index = self.get_index()?;
        self.filtered_entries = index
            .iter()
            .filter_map(|entry| self.process_index_entry(&entry).ok().flatten())
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
            return Err(DotFsError::PathNotFound(path.to_path_buf()).into());
        }
        if !path.starts_with(self.workdir()?) {
            return Err(DotFsError::OutsideWorkTree(path.to_path_buf()).into());
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

    /// Processes an index entry and applies the filter.
    fn process_index_entry(&self, entry: &git2::IndexEntry) -> Result<Option<PathBuf>> {
        let path_str = std::str::from_utf8(&entry.path).map_err(|_| DotFsError::InvalidUtf8Path)?;
        let path = self.workdir()?.join(path_str);
        self.matches_filter(&path)
            .map(|matches| matches.then_some(path))
    }

    /// Retrieves the parent commits for the current HEAD.
    fn get_parent_commits(&self) -> Result<Vec<git2::Commit>> {
        match self.bare.head() {
            Ok(head) => {
                match head.target() {
                    Some(oid) => match self.bare.find_commit(oid) {
                        Ok(commit) => Ok(vec![commit]),
                        Err(e) => Err(e.into()),
                    },
                    None => Ok(vec![]), // Detached HEAD, return empty vec
                }
            }
            Err(_) => Ok(vec![]), // No HEAD, return empty vec
        }
    }

    /// Retrieves the repository index.
    fn get_index(&self) -> Result<Index> {
        Ok(self.bare.index().map_err(DotFsError::Git)?)
    }

    /// Writes the index to disk.
    fn write_index(&self, index: &mut Index) -> Result<()> {
        index.write().map_err(DotFsError::Git)?;
        Ok(())
    }

    /// Gets the working directory of the repository.
    fn workdir(&self) -> Result<&Path> {
        self.bare
            .workdir()
            .ok_or(DotFsError::GitNotInstalled.into())
    }

    /// Computes the relative path from the working directory.
    fn get_relative_path<'a>(&self, path: &'a Path) -> Result<&'a Path> {
        Ok(path.strip_prefix(self.workdir()?)?)
    }

    /// Retrieves the repository status.
    fn repository_status(&self) -> Result<Statuses> {
        let mut opts = git2::StatusOptions::new();
        opts.include_ignored(false)
            .include_untracked(false)
            .include_unmodified(false)
            .show(git2::StatusShow::Index);
        Ok(self.bare.statuses(Some(&mut opts))?)
    }

    /// Verifies that there are staged changes to commit.
    fn verify_staged_changes(&self, statuses: &Statuses) -> Result<()> {
        let has_changes = statuses.iter().any(|entry| {
            // Include both staged changes and tracked file modifications
            entry.status().contains(git2::Status::INDEX_NEW)
                || entry.status().contains(git2::Status::INDEX_MODIFIED)
                || entry.status().contains(git2::Status::INDEX_DELETED)
                || entry.status().contains(git2::Status::WT_MODIFIED)
                || entry.status().contains(git2::Status::WT_NEW)
        });

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

impl Iterator for DotFs {
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
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_file as symlink;
    use std::{env, fs};
    use tempfile::tempdir;
    use tracing::debug;

    //--------------
    // Test Helpers
    //--------------

    struct TestEnv {
        manager: DotFs,
        _temp: tempfile::TempDir,
    }

    impl TestEnv {
        pub fn new() -> Result<Self> {
            // Initialize logging if needed
            init_test_logging();

            // Create a temporary directory
            let temp_dir = tempdir()?;
            let work_tree = temp_dir.path();
            unsafe { env::set_var("HOME", work_tree) };
            let git_dir = work_tree.join(SHELF_BARE_NAME);

            // Initialize a bare repository in the temp directory
            fs::create_dir_all(&git_dir)?;
            let repo = Repository::init_bare(&git_dir)?;
            repo.set_workdir(work_tree, false)?;

            // Create DotFs with the isolated repository
            let manager = DotFs::default();

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

    impl DotFsError {
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

    //------------
    // Test Cases
    //------------

    #[test]
    fn git_installation_detection() -> Result<()> {
        debug!("Testing git installation detection");
        let original_path = env::var_os("PATH");

        // SAFETY: Test environment should run without concurrent access
        unsafe { env::set_var("PATH", "") }
        let err = check_git_installation().unwrap_err();
        let tabs_err = err.downcast_ref::<DotFsError>().unwrap();
        assert!(tabs_err.is_git_not_installed());

        // SAFETY: Test environment should run without concurrent access
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
        let tabs_err = err.downcast_ref::<DotFsError>().unwrap();
        assert!(tabs_err.is_path_not_found());

        // External path
        let external = if cfg!(windows) {
            PathBuf::from("C:\\Windows\\System32\\drivers\\etc\\hosts")
        } else {
            PathBuf::from("/etc/passwd")
        };
        let err = env.manager.track(&[external]).unwrap_err();
        let tabs_err = err.downcast_ref::<DotFsError>().unwrap();
        assert!(tabs_err.is_outside_work_tree());

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

        // NOTE: On Windows, git may normalize line endings, so the status check *after* writing might differ.
        // We re-read the status to check for WT_MODIFIED *after* the write operation.
        let relative_path = test_file.strip_prefix(env.workdir()).unwrap();
        let mut status_opts = git2::StatusOptions::new();
        status_opts.include_untracked(true);
        status_opts.include_ignored(false);

        let status = env.manager.bare.status_file(relative_path)?;
        assert!(
            status.contains(git2::Status::WT_MODIFIED),
            "File should be modified"
        );

        assert_eq!(tracked.len(), 1, "After modification");
        assert_eq!(tracked[0], test_file);

        // Stage changes
        env.manager.track(&[test_file.clone()])?;
        env.manager.save_local_changes()?;

        assert!(env.tracked_paths().is_empty(), "After staging");

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
    fn filter_mode_selection_works() -> Result<()> {
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
