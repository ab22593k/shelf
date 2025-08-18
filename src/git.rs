use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;
use git2::{Commit, DiffOptions, Oid, Repository, Tree};

use crate::error::Shelfor;

/// Helper function to determine the parent commits for a new commit.
///
/// Returns an empty vector if there's no HEAD (e.g., the first commit in a repo
/// or an unborn branch). Otherwise, returns a vector containing the commit
/// pointed to by HEAD. Propagates other `git2` errors.
fn get_parent_commits<'repo>(repo: &'repo Repository) -> Result<Vec<Commit<'repo>>> {
    match repo.head() {
        Ok(head) => Ok(vec![head.peel_to_commit()?]),
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // This is the initial commit for an unborn branch; no parents
            Ok(vec![])
        }
        Err(e) => Err(e.into()),
    }
}

/// Creates a new commit with the given message, including all staged changes.
pub(super) fn commit_action(message: String) -> Result<String> {
    let repo = Repository::open(".").context("Failed to open local git repository")?;
    let signature = repo
        .signature()
        .context("Failed to determine git signature")?;

    // Create a tree from the current index (staged changes).
    let mut index = repo.index().context("Failed to get repository index")?;
    let tree_id = index
        .write_tree()
        .context("Failed to write index to tree")?;
    let tree = repo
        .find_tree(tree_id)
        .context("Failed to find tree from index")?;

    // Determine parent commits, handling the initial commit case.
    let parents = get_parent_commits(&repo)?;
    let parent_references: Vec<&Commit<'_>> = parents.iter().collect();

    // Create the commit.
    repo.commit(
        Some("HEAD"),       // Point HEAD to our new commit
        &signature,         // Author
        &signature,         // Committer
        &message,           // Commit message
        &tree,              // Tree of files
        &parent_references, // Parent commits
    )?;

    println!("{}", "Created git commit successfully".bright_green());
    Ok(message)
}

/// Retrieves recent commit history up to a specified depth.
pub fn commit_history(
    depth: &usize,
    exclude_patterns: Option<&[&str]>,
) -> Result<Vec<(Oid, String)>> {
    let repository = Repository::open(Path::new(".")).context("Failed to open Git repository")?;
    let head = repository.head().context("Failed to get repository HEAD")?;
    let head_commit = head
        .peel_to_commit()
        .context("Failed to peel HEAD to commit")?;

    let mut revwalk = repository
        .revwalk()
        .context("Failed to create revision walker")?;
    revwalk
        .push(head_commit.id())
        .context("Failed to push HEAD to revwalk")?;
    revwalk
        .set_sorting(git2::Sort::TIME)
        .context("Failed to set revwalk sorting")?;

    let history = revwalk
        .take(*depth)
        .filter_map(Result::ok) // Ignore revwalk errors
        .filter_map(|oid| repository.find_commit(oid).ok()) // Ignore errors finding commits
        .filter(|commit| !should_exclude_commit(commit, exclude_patterns))
        .map(|commit| {
            (
                commit.id(),
                commit.message().unwrap_or_default().to_string(),
            )
        })
        .collect::<Vec<_>>();

    Ok(history)
}

/// Determines if a commit should be excluded based on file patterns.
fn should_exclude_commit(commit: &Commit, exclude_patterns: Option<&[&str]>) -> bool {
    let patterns = match exclude_patterns {
        Some(p) => p,
        None => return false,
    };
    let tree = match commit.tree() {
        Ok(t) => t,
        Err(_) => return false, // If we can't get the tree, don't exclude.
    };

    patterns.iter().any(|pattern| {
        tree.iter()
            .any(|entry| entry.name().is_some_and(|name| name.contains(pattern)))
    })
}

/// Verifies that the `git` command is available in the system's PATH.
pub(crate) fn verify_git_installation() -> Result<(), Shelfor> {
    which::which("git").map_err(|_| Shelfor::GitNotInstalled)?;
    Ok(())
}

/// Collects all staged changes and returns them as a formatted diff string.
pub(crate) fn collect_changes() -> Result<String> {
    let repository = Repository::open(Path::new(".")).context("Failed to open Git repository")?;
    let diff = calculate_diff(&repository)?;
    format_diff(&diff)
}

/// Gets the tree for the current HEAD, or an empty tree if HEAD does not exist (e.g., initial commit).
fn get_head_tree<'repo>(repo: &'repo Repository) -> Result<Tree<'repo>> {
    match repo.head() {
        Ok(head) => head.peel_to_tree().context("Failed to peel HEAD to tree"),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // On an unborn branch, there's no HEAD. Use an empty tree for the diff base.
            let builder = repo.treebuilder(None)?;
            let tree_id = builder.write()?;
            repo.find_tree(tree_id)
                .context("Failed to create an empty tree for unborn branch")
        }
        Err(e) => Err(e.into()),
    }
}

/// Gets the tree for the repository's current index (staged files).
fn get_index_tree<'repo>(repo: &'repo Repository) -> Result<Tree<'repo>> {
    let mut index = repo.index().context("Failed to open repository index")?;
    let tree_id = index
        .write_tree()
        .context("Failed to write index to tree")?;
    repo.find_tree(tree_id).context("Failed to find index tree")
}

/// Calculates the diff between the HEAD tree and the index tree (staged changes).
fn calculate_diff(repository: &Repository) -> Result<git2::Diff<'_>> {
    let mut diff_options = DiffOptions::new();
    diff_options
        .context_lines(4)
        .ignore_whitespace_change(true)
        .ignore_whitespace_eol(true);

    let base_tree = get_head_tree(repository)?;
    let index_tree = get_index_tree(repository)?;

    repository
        .diff_tree_to_tree(Some(&base_tree), Some(&index_tree), Some(&mut diff_options))
        .context("Failed to calculate difference between HEAD and index")
}

/// Formats a `git2::Diff` into a standard patch string.
fn format_diff(difference: &git2::Diff) -> Result<String> {
    let mut formatted_difference = String::new();
    difference
        .print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            // The line content from git2 does not include the prefix (+, -, ' ').
            // We reconstruct the line here to form a standard patch format.
            if let Ok(content) = std::str::from_utf8(line.content()) {
                match line.origin() {
                    '+' | '-' | ' ' => {
                        formatted_difference.push(line.origin());
                        formatted_difference.push_str(content);
                    }
                    // For headers ('F', 'H'), etc., the content is the full line.
                    _ => {
                        formatted_difference.push_str(content);
                    }
                }
                true
            } else {
                false // Non-UTF8 content in diff, abort.
            }
        })
        .context("Failed to format difference")?;

    Ok(formatted_difference)
}
