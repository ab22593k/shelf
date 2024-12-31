use anyhow::{anyhow, Context, Result};
use directories::BaseDirs;
use std::{fmt, fs};

pub const USER_COMMIT_PROMPT: &str = r#"Generate concise and professional commit messages. Provide clear context for code changes.

Here is the diff:"#;

pub const USER_REVIEW_PROMPT: &str = r#"Please review the following code changes and provide a detailed analysis. Consider:
1. Potential bugs or issues
2. Code style and best practices
3. Performance implications
4. Security concerns

Here is the diff:"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptKind {
    Commit,
    Review,
}

impl fmt::Display for PromptKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PromptKind::Commit => "commit",
            PromptKind::Review => "review",
        };
        f.write_str(s)
    }
}

impl PromptKind {
    pub fn get_system_prompt(&self) -> Result<String> {
        let path = format!("assets/prompts/{self}.txt");
        Self::load_prompt_from(&path)
    }

    pub fn get_user_prompt(&self) -> Result<&'static str> {
        match self {
            PromptKind::Commit => Ok(USER_COMMIT_PROMPT),
            PromptKind::Review => Ok(USER_REVIEW_PROMPT),
        }
    }

    fn load_prompt_from(path: &str) -> Result<String> {
        let config_dir = BaseDirs::new()
            .map(|p| p.config_dir().join("shelf").join(path))
            .ok_or_else(|| anyhow!("Could not determine data directory"))?;

        fs::read_to_string(&config_dir)
            .with_context(|| format!("Failed to read prompt file: {}", path))
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
        let path = file.path().to_str().unwrap();
        let prompt = PromptKind::load_prompt_from(path)?;
        assert_eq!(prompt, "test prompt");
        Ok(())
    }

    #[test]
    fn test_prompt_kind_display() {
        assert_eq!(PromptKind::Commit.to_string(), "commit");
        assert_eq!(PromptKind::Review.to_string(), "review");
    }
}
