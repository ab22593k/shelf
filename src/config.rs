use crate::gitai::GitAIConfig;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShelfConfig {
    path: PathBuf,
}

impl ShelfConfig {
    pub fn get_path(self) -> PathBuf {
        self.path
    }

    fn init_xdg_config_home() -> PathBuf {
        let path = directories::BaseDirs::new()
            .map(|base_dirs| base_dirs.config_dir().join("shelf"))
            .or_else(|| {
                std::env::var("XDG_CONFIG_HOME")
                    .ok()
                    .map(|x| PathBuf::from(x).join("shelf"))
            })
            .or_else(|| home::home_dir().map(|x| x.join(".config").join("shelf")))
            .unwrap_or_else(|| std::env::current_dir().unwrap().join(".shelf"));

        path
    }

    pub fn dotconf_conf() -> Self {
        Self {
            path: Self::init_xdg_config_home().join("dotconf.db"),
        }
    }

    pub fn gitai_conf() -> PathBuf {
        Self::init_xdg_config_home().join("gitai.json")
    }

    pub fn load_config() -> Result<GitAIConfig> {
        let config_path = Self::gitai_conf();

        if config_path.exists() {
            let content = fs::read_to_string(&config_path).with_context(|| {
                format!("Failed to read config file: {}", config_path.display())
            })?;

            serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {}", config_path.display()))
        } else {
            // Create default config and save it
            let config = GitAIConfig::default();
            Self::save_config(&config)?;
            Ok(config)
        }
    }

    pub fn save_config(config: &GitAIConfig) -> Result<()> {
        let config_path = Self::gitai_conf();

        // Create parent directories if they don't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        // Serialize and save config
        let content = serde_json::to_string_pretty(config).context("Failed to serialize config")?;

        let mut file = fs::File::create(&config_path)
            .with_context(|| format!("Failed to create config file: {}", config_path.display()))?;

        file.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

        Ok(())
    }
}
