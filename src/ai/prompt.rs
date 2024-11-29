use anyhow::{anyhow, Result};
use std::fs;

use std::fmt;

pub enum PromptKind {
    Commit,
    Review,
}

impl fmt::Display for PromptKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PromptKind::Commit => write!(f, "commit.prompt"),
            PromptKind::Review => write!(f, "review.prompt"),
        }
    }
}

macro_rules! prompt_path {
    (user, $kind:expr) => {
        format!("assets/user_{}", $kind)
    };
    (system, $kind:expr) => {
        format!("assets/system_{}", $kind)
    };
}

impl PromptKind {
    pub fn get_system_prompt(&self) -> Result<String> {
        load_prompt_from_path(&prompt_path!(system, self))
    }

    pub fn get_user_prompt(self) -> Result<String> {
        load_prompt_from_path(&prompt_path!(user, self))
    }
}

fn load_prompt_from_path(path: &str) -> Result<String> {
    let config_dir = directories::BaseDirs::new()
        .map(|p| p.config_dir().join("shelf").join(path))
        .ok_or_else(|| anyhow!("Could not determine data directory"))?;

    match fs::read_to_string(&config_dir) {
        Ok(content) => Ok(content),
        Err(e) => Err(anyhow!("Failed to read prompt file {}", path).context(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{error::Error, io::Write};
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_prompt() -> Result<(), Box<dyn Error>> {
        let mut file = NamedTempFile::new()?;
        file.write_all(b"test prompt")?;
        let path = file.path();
        let prompt = load_prompt_from_path(path.to_str().unwrap())?;
        assert_eq!(prompt, "test prompt");
        Ok(())
    }
}
