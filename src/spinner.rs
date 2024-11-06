use std::{future::Future, time::Duration};

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

pub async fn embed_spinner<F, Fut, T>(callback: F) -> Result<T>
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

    let result = callback().await?;
    spinner.finish_and_clear();

    Ok(result)
}
