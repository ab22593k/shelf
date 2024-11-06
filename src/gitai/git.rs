use anyhow::Result;

pub fn git_diff(repo: &git2::Repository) -> Result<String> {
    let mut diff_str = String::new();
    let mut index = repo.index()?;
    index.read(true)?;

    // Generate diff between HEAD (or empty tree) and index
    let old_tree = match repo.head() {
        Ok(head) => Some(head.peel_to_tree()?),
        Err(_) => {
            // For initial commit, use empty tree
            let empty_oid = repo.treebuilder(None)?.write()?;
            Some(repo.find_tree(empty_oid)?)
        }
    };

    // Configure diff options
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .context_lines(3);

    // Generate the diff
    let diff = repo.diff_tree_to_index(old_tree.as_ref(), Some(&index), Some(&mut opts))?;

    // Add summary stats
    let stats = diff.stats()?;
    diff_str.push_str(&format!(
        "Changes staged for commit ({} files changed, {} insertions(+), {} deletions(-))\n\n",
        stats.files_changed(),
        stats.insertions(),
        stats.deletions()
    ));

    // Track current file for grouping changes
    let mut current_file = None;
    let mut current_content = String::new();

    diff.print(git2::DiffFormat::Patch, |delta, hunk, line| {
        // Handle new file
        if let Some(file) = delta.new_file().path() {
            let file_path = file.display().to_string();

            // Write previous file content if switching files
            if current_file.as_ref() != Some(&file_path) {
                if !current_content.is_empty() {
                    diff_str.push_str(&current_content);
                    current_content.clear();
                }
                current_file = Some(file_path.clone());
                current_content.push_str(&format!("\ndiff --git a/{0} b/{0}\n", file_path));

                // Add file metadata
                if delta.status() == git2::Delta::Added {
                    current_content.push_str("new file mode 100644\n");
                } else if delta.status() == git2::Delta::Deleted {
                    current_content.push_str("deleted file mode 100644\n");
                }
            }
        }

        // Add hunk header
        if let Some(hunk) = hunk {
            current_content.push_str(&format!("{}", String::from_utf8_lossy(hunk.header())));
        }

        // Add line content with proper prefix
        let content = String::from_utf8_lossy(line.content());
        match line.origin() {
            '+' => current_content.push_str(&format!("+{}", content)),
            '-' => current_content.push_str(&format!("-{}", content)),
            ' ' => current_content.push_str(&format!(" {}", content)),
            'F' => current_content.push_str(&format!("File {}", content)),
            'H' => current_content.push_str(&format!("Hunk {}", content)),
            _ => {}
        }

        true
    })?;

    // Add final file content
    if !current_content.is_empty() {
        diff_str.push_str(&current_content);
    }

    Ok(diff_str)
}

pub fn git_diff_cached(repo: &git2::Repository) -> Result<String> {
    let mut diff_str = String::new();
    let mut index = repo.index()?;
    index.read(true)?;

    // Get HEAD tree or empty tree for initial commit
    let head_tree = if let Ok(head) = repo.head() {
        Some(head.peel_to_tree()?)
    } else {
        let empty_oid = repo.treebuilder(None)?.write()?;
        Some(repo.find_tree(empty_oid)?)
    };

    // Configure diff options for better context
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        .context_lines(3)
        .show_untracked_content(true)
        .indent_heuristic(true)
        .patience(true);

    // Get changes between HEAD and index
    let diff = repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut opts))?;

    let stats = diff.stats()?;
    if stats.files_changed() > 0 {
        // Add human-readable summary
        diff_str.push_str(&format!(
            "Summary: {} files were modified with {} lines added and {} lines removed\n\n",
            stats.files_changed(),
            stats.insertions(),
            stats.deletions()
        ));

        // Track current file and changes
        let mut current_file = None;
        let mut file_changes = String::new();

        diff.print(git2::DiffFormat::Patch, |delta, hunk, line| {
            // Add file context
            if let Some(file) = delta.new_file().path() {
                let file_path = file.display().to_string();
                if current_file.as_ref() != Some(&file_path) {
                    if !file_changes.is_empty() {
                        diff_str.push_str(&file_changes);
                        file_changes.clear();
                    }
                    current_file = Some(file_path.clone());
                    file_changes.push_str(&format!("\nIn file '{}':\n", file_path));
                }
            }

            true
        })?;

        // Add final file changes
        if !file_changes.is_empty() {
            diff_str.push_str(&file_changes);
        }
    }

    Ok(diff_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;
    use tempfile::TempDir;

    fn setup_test_repo() -> Result<(TempDir, Repository)> {
        let temp_dir = TempDir::new()?;
        let repo = Repository::init(&temp_dir)?;

        // Configure test user
        let mut config = repo.config()?;
        config.set_str("user.name", "Test User")?;
        config.set_str("user.email", "test@example.com")?;

        Ok((temp_dir, repo))
    }

    #[test]
    fn test_empty_repo_no_changes() -> Result<()> {
        let (_tmpd, repo) = setup_test_repo()?;
        let changes = git_diff(&repo)?;
        assert!(changes.contains("0 files changed"));
        Ok(())
    }

    #[test]
    fn test_single_staged_file() -> Result<()> {
        let (_tmpd, repo) = setup_test_repo()?;
        let root = repo.workdir().unwrap();

        // Create a file
        let test_file = root.join("test.txt");
        fs::write(&test_file, "test content\n")?;

        // Stage the file
        let mut index = repo.index()?;
        index.add_path(Path::new("test.txt"))?;
        index.write()?;

        let changes = git_diff(&repo)?;
        assert!(changes.contains("1 files changed"), "changes: {}", changes);
        assert!(changes.contains("test.txt"), "changes: {}", changes);
        assert!(changes.contains("+test content"), "changes: {}", changes);

        Ok(())
    }

    #[test]
    fn test_modified_staged_file() -> Result<()> {
        let (_tmpd, repo) = setup_test_repo()?;
        let root = repo.workdir().unwrap();

        // Create and commit initial file
        let test_file = root.join("test.txt");
        fs::write(&test_file, "initial content\n")?;

        let mut index = repo.index()?;
        index.add_path(Path::new("test.txt"))?;
        index.write()?;

        let tree_id = index.write_tree()?;
        let sig = git2::Signature::now("Test User", "test@example.com")?;
        let tree = repo.find_tree(tree_id)?;
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;

        // Modify and stage the file
        fs::write(&test_file, "modified content\n")?;
        index.add_path(Path::new("test.txt"))?;
        index.write()?;

        let changes = git_diff_cached(&repo)?;
        assert!(changes.contains("1 files were modified"));
        assert!(changes.contains("1 lines added"));
        assert!(changes.contains("1 lines removed"));
        assert!(changes.contains("In file 'test.txt'"));

        Ok(())
    }

    #[test]
    fn test_multiple_file_changes() -> Result<()> {
        let (_tmpd, repo) = setup_test_repo()?;
        let root = repo.workdir().unwrap();

        // Create and stage multiple files with lots of content
        for i in 1..=3 {
            let file_name = format!("file_{}.txt", i);
            let file_path = root.join(&file_name);
            let content = (1..=9)
                .map(|n| format!("Line {} of file {}\n", n, i))
                .collect::<String>();

            fs::write(&file_path, &content)?;

            let mut index = repo.index()?;
            index.add_path(Path::new(&file_name))?;
            index.write()?;
        }

        let changes = git_diff(&repo)?;
        assert!(changes.contains("3 files changed"), "changes: {}", changes);
        assert!(changes.contains("27 insertions"), "changes: {}", changes);
        assert!(changes.contains("file_1.txt"), "changes: {}", changes);
        assert!(changes.contains("file_2.txt"), "changes: {}", changes);
        assert!(changes.contains("file_3.txt"), "changes: {}", changes);
        assert!(changes.contains("Line 1 of file 1"), "changes: {}", changes);
        assert!(changes.contains("Line 9 of file 3"), "changes: {}", changes);

        Ok(())
    }
}
