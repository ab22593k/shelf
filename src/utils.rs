use std::{future::Future, path::Path, time::Duration};

use anyhow::{Context, Result};
use colored::Colorize;
use git2::{DiffOptions, Repository};
use indicatif::{ProgressBar, ProgressStyle};

use crate::dotfs::TabsError;

pub async fn run_with_progress<F, Fut, T>(op: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈")
            .template("{spinner} Generating commit message...")?,
    );
    spinner.enable_steady_tick(Duration::from_millis(120));

    let result = op().await?;
    spinner.finish_and_clear();
    Ok(result)
}

/// Prints a styled success message.
pub fn print_success(message: &str) {
    println!("{} {}", "✓".bright_green(), message.bold().green());
}

pub fn check_git_installation() -> Result<bool> {
    which::which("git").map_err(|_| TabsError::GitNotInstalled)?;
    Ok(true)
}

pub fn get_staged_diff() -> Result<String> {
    let repo = Repository::open(Path::new(".")).context("Opening git repository failed")?;
    let diff = generate_diff(&repo)?;

    format_diff(&diff)
}

fn generate_diff(repo: &Repository) -> Result<git2::Diff<'_>> {
    let mut diff_opts = DiffOptions::new();
    diff_opts.context_lines(3).patience(true).minimal(true);

    // Get head tree and index tree
    let head_tree = match repo.head() {
        Ok(head) => head.peel_to_tree().context("Getting HEAD tree failed")?,
        Err(_) => {
            // Handle first commit case where HEAD doesn't exist yet

            repo.find_tree(repo.treebuilder(None)?.write()?)?
        }
    };

    let index_tree = repo
        .index()
        .context("Getting repo index failed")?
        .write_tree()
        .context("Writing index tree failed")?;
    let index_tree = repo
        .find_tree(index_tree)
        .context("Finding index tree failed")?;

    // Generate diff between head and index trees
    repo.diff_tree_to_tree(Some(&head_tree), Some(&index_tree), Some(&mut diff_opts))
        .context("Generating diff failed")
}

fn format_diff(diff: &git2::Diff) -> Result<String> {
    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        if let Ok(text) = std::str::from_utf8(line.content()) {
            match line.origin() {
                '+' => diff_text.push('+'),
                '-' => diff_text.push('-'),
                _ => {}
            }
            diff_text.push_str(text);
            true
        } else {
            false
        }
    })
    .context("Formatting diff failed")?;

    Ok(diff_text)
}
