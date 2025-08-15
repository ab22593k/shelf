use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;
use git2::{Commit, Oid, Repository};

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

pub(super) fn commit_action(message: String) -> Result<String> {
    // Open the current Git repository
    let repo = Repository::open(".")?;
    let signature = repo.signature()?;

    // Prepare the tree for the commit.
    // The index needs to be mutable to write the tree from its current state.
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    // Determine the parent commits for the new commit.
    // This handles the case of the very first commit in a repository.
    let parents = get_parent_commits(&repo)?;

    // Collect references to parent commits as required by git2::Repository::commit.
    let parents_refs: Vec<&Commit<'_>> = parents.iter().collect();

    // Create the new commit.
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        parents_refs.as_slice(),
    )?;

    // Inform the user of successful commit creation
    println!("{}", "Created git commit successfully".bright_green());
    Ok(message)
}

pub fn commit_history(
    saga_depth: &usize,
    evade_patterns: Option<&[&str]>,
) -> Result<Vec<(Oid, String)>> {
    let repository =
        Repository::open(Path::new(".")).context("Unearthing git repository failed")?;
    let apex_commit = repository
        .head()
        .context("Seeking repository HEAD failed")?
        .peel_to_commit()
        .context("Seeking HEAD commit failed")?;
    let mut chronicle_walker = repository
        .revwalk()
        .context("Weaving revision chronicle failed")?;
    chronicle_walker
        .push(apex_commit.id())
        .context("Anchoring starting commit failed")?;
    chronicle_walker
        .set_sorting(git2::Sort::TIME)
        .context("Setting chronicle order failed")?;

    let mut saga_chapters = Vec::new();
    for commit_id in chronicle_walker.take(*saga_depth) {
        match commit_id {
            Ok(oid) => match repository.find_commit(oid) {
                Ok(commit) => {
                    if !should_escape_commit(&commit, evade_patterns) {
                        saga_chapters.push((oid, commit.message().unwrap_or_default().to_string()));
                    }
                }
                Err(err) => {
                    eprintln!("Failed to find commit {oid}: {err}");
                }
            },
            Err(err) => {
                eprintln!("Invalid commit ID: {err}");
            }
        }
    }
    Ok(saga_chapters)
}

fn should_escape_commit(commit: &Commit, escape_patterns: Option<&[&str]>) -> bool {
    if let (Some(patterns), Ok(file_tree)) = (escape_patterns, commit.tree()) {
        for pattern in patterns {
            if file_tree.iter().any(|entry| {
                entry
                    .name()
                    .map(|name| name.contains(pattern))
                    .unwrap_or(false)
            }) {
                return true;
            }
        }
    }
    false
}
