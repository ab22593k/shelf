use anyhow::{anyhow, Result};
use std::fs;

use std::fmt;

pub const USER_COMMIT_PROMPT: &str =
    "Generate concise and professional commit messages.  Provide clear context for code changes.

Git diff changes:
";

pub const USER_REVIEW_PROMPT: &str =
    "Please review the following code changes and provide a detailed analysis. Consider:
    1. Potential bugs or issues
    2. Code style and best practices
    3. Performance implications
    4. Security concerns

    Here is the diff:";

pub enum SysPromptKind {
    Commit,
    Review,
}

impl fmt::Display for SysPromptKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SysPromptKind::Commit => write!(f, "commit.prompt"),
            SysPromptKind::Review => write!(f, "review.prompt"),
        }
    }
}

macro_rules! assets {
    ($kind:expr) => {
        format!("assets/system/{}", $kind)
    };
}

impl SysPromptKind {
    pub fn get_system_prompt(&self) -> Result<String> {
        Self::load_prompt_from(&assets!(self))
    }

    pub fn get_user_prompt(self) -> Result<String> {
        match self {
            SysPromptKind::Commit => Ok(USER_COMMIT_PROMPT.to_string()),
            SysPromptKind::Review => Ok(USER_REVIEW_PROMPT.to_string()),
        }
    }

    fn load_prompt_from(path: &str) -> Result<String> {
        let config_dir = directories::BaseDirs::new()
            .map(|p| p.config_dir().join("shelf").join(path))
            .ok_or_else(|| anyhow!("Could not determine data directory"))?;

        match fs::read_to_string(&config_dir) {
            Ok(content) => Ok(content),
            Err(e) => Err(anyhow!("Failed to read prompt file {}", path).context(e)),
        }
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
        let prompt = SysPromptKind::load_prompt_from(path.to_str().unwrap())?;
        assert_eq!(prompt, "test prompt");
        Ok(())
    }
}
