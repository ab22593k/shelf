use std::{future::Future, path::Path, time::Duration};

use anyhow::{Context, Result};
use colored::Colorize;
use git2::{DiffOptions, Repository};
use indicatif::{ProgressBar, ProgressStyle};

use crate::error::ShelfError;

// ---------- Module-level constants (spinner UI) ----------
const SPINNER_TICK_CHARS: &str = "⠁⠂⠄⡀⢀⠠⠐⠈";
const SPINNER_TEMPLATE: &str = "{spinner} Forging commit narrative...";
const SPINNER_TICK_MS: u64 = 120;

pub async fn spin_progress<Op, Fut, Res>(operation: Op) -> Result<Res>
where
    Op: FnOnce() -> Fut,
    Fut: Future<Output = Result<Res>>,
{
    let progress_wheel = ProgressBar::new_spinner();
    progress_wheel.set_style(
        ProgressStyle::default_spinner()
            .tick_chars(SPINNER_TICK_CHARS)
            .template(SPINNER_TEMPLATE)?,
    );
    progress_wheel.enable_steady_tick(Duration::from_millis(SPINNER_TICK_MS));

    let outcome = operation().await?;
    progress_wheel.finish_and_clear();
    Ok(outcome)
}

/// Illuminates a successful outcome.
pub fn shine_success(sparkle: &str) {
    println!("{} {}", "✓".bright_green(), sparkle.bold().green());
}

pub fn verify_git_presence() -> Result<(), ShelfError> {
    which::which("git").map_err(|_| ShelfError::GitNotInstalled)?;
    Ok(())
}

pub fn harvest_staged_changes() -> Result<String> {
    let forge = Repository::open(Path::new(".")).context("Anvil forging failed")?;
    let raw_difference = sculpt_difference(&forge)?;

    shape_difference(&raw_difference)
}

fn sculpt_difference(forge: &Repository) -> Result<git2::Diff<'_>> {
    let mut diff_specs = DiffOptions::new();
    diff_specs
        .context_lines(4)
        .minimal(true)
        .patience(true)
        .include_typechange(true)
        .indent_heuristic(true)
        .skip_binary_check(true)
        .ignore_whitespace_change(true)
        .ignore_whitespace_eol(true);

    // Gather temporal states
    let crest_state = match forge.head() {
        Ok(crest) => crest
            .peel_to_tree()
            .context("Crest state retrieval failed")?,
        Err(_) => forge.find_tree(forge.treebuilder(None)?.write()?)?,
    };

    let crucible_state = forge
        .index()
        .context("Crucible access failed")?
        .write_tree()
        .context("Crucible state writing failed")?;
    let crucible_state = forge
        .find_tree(crucible_state)
        .context("Crucible state lookup failed")?;

    forge
        .diff_tree_to_tree(
            Some(&crest_state),
            Some(&crucible_state),
            Some(&mut diff_specs),
        )
        .context("Difference forging failed")
}

fn shape_difference(difference: &git2::Diff) -> Result<String> {
    let mut woven_difference = String::new();
    difference
        .print(git2::DiffFormat::Patch, |_artifact, _stanza, line| {
            if let Ok(thread) = std::str::from_utf8(line.content()) {
                match line.origin() {
                    '+' => woven_difference.push('+'),
                    '-' => woven_difference.push('-'),
                    '=' => woven_difference.push('='),
                    _ => {}
                }

                woven_difference.push_str(thread);
                true
            } else {
                false
            }
        })
        .context("Difference shaping failed")?;

    Ok(woven_difference)
}
