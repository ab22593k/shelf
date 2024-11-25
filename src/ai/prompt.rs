use std::fs;
use std::path::Path;

use anyhow::Result;

pub enum PromptKind {
    Commit,
    Review,
}

pub fn get_user_prompt(kind: &PromptKind) -> Result<String> {
    match kind {
        PromptKind::Commit => load_prompt("assets/user/commit.txt"),
        PromptKind::Review => load_prompt("assets/user/review.txt"),
    }
}

pub fn get_system_prompt(kind: &PromptKind) -> Result<String> {
    match kind {
        PromptKind::Commit => load_prompt("assets/system/commit.txt"),
        PromptKind::Review => load_prompt("assets/system/review.txt"),
    }
}

fn load_prompt(path: impl AsRef<Path>) -> Result<String> {
    let fallback = dirs::data_local_dir()
        .map(|p| p.join("shelf"))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine data directory",
            )
        })?;

    let data_home = std::env::var("XDG_DATA_HOME")
        .map(|s| Path::new(&s).join("shelf"))
        .unwrap_or(fallback);

    Ok(fs::read_to_string(data_home.join(path))
        .map_err(|e| std::io::Error::new(e.kind(), format!("Failed to read prompt file: {}", e)))?)
}
