use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dotfile {
    pub name: String,
    pub source: PathBuf,
    pub target_dir: PathBuf,
}

impl Dotfile {
    pub async fn new<P: AsRef<Path>>(source: P, target_dir: PathBuf) -> Result<Self> {
        let source = source.as_ref().to_path_buf();
        if !source.exists() {
            return Err(anyhow::anyhow!(
                "Source file does not exist: {}",
                source.display()
            ));
        }
        let source = fs::canonicalize(&source)
            .await
            .context("Failed to canonicalize source path")?;

        if !fs::metadata(&source).await.is_ok() {
            return Err(anyhow::anyhow!(
                "Source file does not exist: {}",
                source.display()
            ));
        }

        let name = source
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?
            .to_string();

        Ok(Self {
            name,
            source,
            target_dir,
        })
    }

    pub async fn create_symlink(&self) -> Result<()> {
        let target = self.target_dir.join(&self.name);

        if fs::metadata(&target).await.is_ok() {
            let metadata = fs::metadata(&target).await?;
            if metadata.is_dir() {
                fs::remove_dir_all(&target)
                    .await
                    .context("Failed to remove existing directory")?;
            } else {
                fs::remove_file(&target)
                    .await
                    .context("Failed to remove existing file")?;
            }
        }

        #[cfg(unix)]
        tokio::fs::symlink(&self.source, &target)
            .await
            .context("Failed to create symlink")?;

        #[cfg(windows)]
        tokio::fs::symlink_file(&self.source, &target)
            .await
            .context("Failed to create symlink")?;

        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn target(&self) -> PathBuf {
        self.target_dir.join(&self.name)
    }
}
