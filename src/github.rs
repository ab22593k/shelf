use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RemoteRepos {
    Github,
    // Add other repository types here in the future
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHost {
    repo: RemoteRepos,
    token: String,
}

impl RemoteHost {
    pub fn new(repo: RemoteRepos, token: String) -> Self {
        let cred = match repo {
            RemoteRepos::Github => RemoteHost { repo, token },
            // Add other cases here as new RemoteRepos variants are added
        };

        cred.validate();
        cred
    }

    pub fn from_env() -> Result<Self> {
        match RemoteRepos::Github {
            RemoteRepos::Github => {
                let token = std::env::var("GH_TOKEN")
                    .context("GH_TOKEN environment variable is not set")?;
                Ok(Self::new(RemoteRepos::Github, token))
            } // Add other cases here as new RemoteRepos variants are added
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self.repo {
            RemoteRepos::Github => {
                if self.token.is_empty() {
                    return Err(anyhow!("GitHub token cannot be empty"));
                }
            } // Add other cases here as new RemoteRepos variants are added
        }

        Ok(())
    }

    pub fn set_env(&self) -> Result<()> {
        match self.repo {
            RemoteRepos::Github => std::env::set_var("GH_TOKEN", &self.token),
            // Add other cases here as new RemoteRepos variants are added
        }

        Ok(())
    }

    pub fn get_repo(&self) -> &RemoteRepos {
        &self.repo
    }

    pub fn get_token(&self) -> &str {
        &self.token
    }
}
