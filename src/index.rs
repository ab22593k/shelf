use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::dotfile::Dotfile;

#[derive(Debug, Serialize, Deserialize)]
pub struct SlfIndex {
    dotfiles: HashMap<String, Dotfile>,
    target_directory: PathBuf,
}

impl SlfIndex {
    pub async fn new<P: AsRef<Path>>(target_directory: P) -> Result<Self> {
        let target_directory = target_directory.as_ref().to_path_buf();
        fs::create_dir_all(&target_directory)
            .await
            .context("Failed to create target directory")?;

        Ok(Self {
            dotfiles: HashMap::new(),
            target_directory,
        })
    }
    pub async fn add_ref<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path"))?;
        let expanded_path = shellexpand::tilde(path_str);
        let path_buf = PathBuf::from(expanded_path.as_ref());

        if !path_buf.exists() {
            return Err(anyhow::anyhow!(
                "File does not exist: {}",
                path_buf.display()
            ));
        }

        let dotfile = Dotfile::new(path_buf.clone(), self.target_directory.clone()).await?;
        let name = dotfile.name().to_string();

        if self.dotfiles.contains_key(&name) {
            return Err(anyhow::anyhow!(
                "Dotfile already exists: {}",
                path_buf.display()
            ));
        }

        self.dotfiles.insert(name, dotfile);
        Ok(())
    }

    pub fn remove_ref<S: AsRef<str>>(&mut self, name: S) -> Result<()> {
        let name = name.as_ref();
        self.dotfiles
            .remove(name)
            .ok_or_else(|| anyhow::anyhow!("Dotfile not found: {}", name))?;
        Ok(())
    }
    pub fn list(&self) -> impl Iterator<Item = (&String, &Dotfile)> {
        let mut sorted_keys: Vec<_> = self.dotfiles.keys().collect();
        sorted_keys.sort();
        sorted_keys
            .into_iter()
            .map(|key| (key, &self.dotfiles[key]))
    }

    pub async fn do_sync(&self) -> Result<()> {
        let target_dir = self.target_directory.clone();
        // .map(|p| p.as_ref().to_path_buf())
        // .unwrap_or_else(|| self.target_directory.clone());

        fs::create_dir_all(&target_dir)
            .await
            .context("Failed to create output directory")?;

        for dotfile in self.dotfiles.values() {
            let target_path = target_dir.join(dotfile.name());
            dotfile
                .create_symlink()
                .await
                .with_context(|| format!("Failed to sync dotfile: {}", dotfile.name()))?;
            println!(
                "Synced: {} -> {}",
                dotfile.source().display(),
                target_path.display()
            );
        }
        Ok(())
    }
}
