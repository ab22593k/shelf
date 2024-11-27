use anyhow::{anyhow, Result};
use colored::Colorize;
use std::{fs, path::Path};

pub fn get_diff_cached<T: AsRef<Path>>(path: T) -> Result<String> {
    let repo = git2::Repository::open(path)?;
    let mut options = git2::DiffOptions::new();
    options.include_typechange_trees(true);

    let index = repo.index()?;
    let head_tree = repo.head()?.peel_to_tree()?;
    let diff = repo.diff_tree_to_index(Some(&head_tree), Some(&index), Some(&mut options))?;

    let mut diff_string = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let content = match line.origin() {
            '+' => format!("+{}", String::from_utf8_lossy(line.content())),
            '-' => format!("-{}", String::from_utf8_lossy(line.content())),
            _ => String::from_utf8_lossy(line.content()).to_string(),
        };
        diff_string.push_str(&content);
        true
    })?;

    Ok(diff_string)
}

pub fn install_git_hook(hooks_dir: &Path) -> Result<()> {
    fs::create_dir_all(hooks_dir)?;
    let hook_path = hooks_dir.join("prepare-commit-msg");
    let current_exe = std::env::current_exe()?;

    // Get the hook binary path relative to the current executable
    let hook_binary = current_exe
        .parent()
        .ok_or_else(|| anyhow!("Cannot determine executable directory"))?
        .join("ai-hook");

    let hook_content = format!(
        r#"#!/bin/sh
# Generated by shelf ai
exec {} "$@""#,
        hook_binary.display()
    );

    fs::write(&hook_path, hook_content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms)?;
    }

    println!("{}", "Git hook installed successfully.".green());
    Ok(())
}

pub fn remove_git_hook(hooks_dir: &Path) -> Result<()> {
    let hook_path = hooks_dir.join("prepare-commit-msg");
    if hook_path.exists() {
        fs::remove_file(hook_path)?;
    }

    println!("{}", "Git hook removed successfully.".green());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_install_git_hook() -> Result<()> {
        let temp_dir = tempdir()?;
        let hooks_dir = temp_dir.path().join(".git").join("hooks");
        install_git_hook(&hooks_dir)?;
        assert!(hooks_dir.join("prepare-commit-msg").exists());
        Ok(())
    }

    #[test]
    fn test_remove_git_hook() -> Result<()> {
        let temp_dir = tempdir()?;
        let hooks_dir = temp_dir.path().join(".git").join("hooks");
        fs::create_dir_all(&hooks_dir)?;
        fs::write(hooks_dir.join("prepare-commit-msg"), "")?;
        remove_git_hook(&hooks_dir)?;
        assert!(!hooks_dir.join("prepare-commit-msg").exists());
        Ok(())
    }

    #[test]
    fn test_get_diff_cached() -> Result<()> {
        let temp_dir = tempdir()?;
        let repo = Repository::init(temp_dir.path())?;
        let mut index = repo.index()?;
        let file_path = temp_dir.path().join("file.txt");
        fs::write(&file_path, "initial content")?;
        index.add_path(Path::new("file.txt"))?;
        index.write()?;

        // Create an initial commit so there is a HEAD
        let signature = git2::Signature::now("Author Name", "author@test.com")?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "initial commit",
            &tree,
            &[],
        )?;

        // Modify the file and add the changes to the index
        fs::write(&file_path, "modified content")?;
        index.add_path(Path::new("file.txt"))?;
        index.write()?;

        let diff_string = get_diff_cached(temp_dir.path())?;
        assert!(diff_string.contains("modified content"));
        assert!(diff_string.contains("+modified content"));
        assert!(diff_string.contains("-initial content"));

        Ok(())
    }
}