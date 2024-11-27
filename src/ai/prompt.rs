use anyhow::Result;
use std::fs;

pub enum PromptKind {
    Commit,
    Review,
}

impl PromptKind {
    pub fn get_user_prompt(&self) -> Result<String> {
        self.load_prompt("assets/user")
    }

    pub fn get_system_prompt(&self) -> Result<String> {
        self.load_prompt("assets/system")
    }

    fn load_prompt(&self, base_path: &str) -> Result<String> {
        let path = match self {
            PromptKind::Commit => base_path.to_owned() + "/commit.txt",
            PromptKind::Review => base_path.to_owned() + "/review.txt",
        };

        load_prompt_from_path(&path)
    }
}

fn load_prompt_from_path(path: &str) -> Result<String> {
    let config_dir = directories::BaseDirs::new()
        .map(|p| p.config_dir().join("shelf").join(path))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine data directory",
            )
        })?;

    Ok(fs::read_to_string(config_dir)
        .map_err(|e| std::io::Error::new(e.kind(), format!("Failed to read prompt file: {}", e)))?)
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
