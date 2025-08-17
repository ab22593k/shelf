use std::{future::Future, time::Duration};

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

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

pub fn shine_success(sparkle: &str) {
    println!("{} {}", "✓".bright_green(), sparkle.bold().green());
}
