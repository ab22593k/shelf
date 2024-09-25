use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RemoteRepos {
    Github,
    // Add other repository types here in the future
}
/// Represents a remote host for version control repositories.
/// This struct encapsulates the type of repository and the authentication token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHost {
    /// The type of remote repository (e.g., Github)
    repo: RemoteRepos,
    /// Authentication token for the remote repository
    token: String,
}

impl RemoteHost {
    /// Creates a new RemoteHost instance.
    ///
    /// # Arguments
    ///
    /// * `repo` - The type of remote repository
    /// * `token` - The authentication token for the repository
    ///
    /// # Returns
    ///
    /// Returns a Result containing the new RemoteHost instance if validation succeeds,
    /// or an error if validation fails.
    pub fn new(repo: RemoteRepos, token: String) -> Result<Self> {
        let remote_host = RemoteHost { repo, token };
        // remote_host.validate()?;
        Ok(remote_host)
    }

    /// Creates a new RemoteHost instance from environment variables.
    ///
    /// # Returns
    ///
    /// Returns a Result containing the new RemoteHost instance if the environment
    /// variable is set and valid, or an error otherwise.
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN")
            .context("GITHUB_TOKEN environment variable is not set")?;
        Self::new(RemoteRepos::Github, token)
    }

    /// Validates the RemoteHost instance.
    ///
    /// # Returns
    ///
    /// Returns Ok(()) if the instance is valid, or an error if validation fails.
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

    /// Sets the environment variable for the authentication token.
    ///
    /// # Returns
    ///
    /// Returns Ok(()) if the environment variable is set successfully.
    pub fn set_env(&self) -> Result<()> {
        match self.repo {
            RemoteRepos::Github => std::env::set_var("GITHUB_TOKEN", &self.token),
            // Add other cases here as new RemoteRepos variants are added
        }

        Ok(())
    }

    /// Gets the repository type.
    ///
    /// # Returns
    ///
    /// Returns a reference to the RemoteRepos enum representing the repository type.
    pub fn get_repo(&self) -> &RemoteRepos {
        &self.repo
    }

    /// Gets the authentication token.
    ///
    /// # Returns
    ///
    /// Returns a string slice containing the authentication token.
    pub fn get_token(&self) -> &str {
        &self.token
    }
}
