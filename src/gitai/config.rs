use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub fn get_config_path(custom_path: Option<PathBuf>) -> PathBuf {
    custom_path.unwrap_or_else(|| {
        directories::BaseDirs::new()
            .map(|base_dirs| base_dirs.config_dir().join("shelf").join("gitai.json"))
            .or_else(|| {
                std::env::var("XDG_CONFIG_HOME")
                    .ok()
                    .map(|x| PathBuf::from(x).join("shelf").join("gitai.json"))
            })
            .or_else(|| {
                home::home_dir().map(|x| x.join(".config").join("shelf").join("gitai.json"))
            })
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap()
                    .join(".shelf")
                    .join("gitai.json")
            })
    })
}

pub fn load_config(custom_path: Option<PathBuf>) -> Result<super::GitAIConfig> {
    let config_path = get_config_path(custom_path);

    if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))
    } else {
        // Create default config and save it
        let config = super::GitAIConfig::default();
        save_config(&config, Some(config_path))?;
        Ok(config)
    }
}

pub fn save_config(config: &super::GitAIConfig, custom_path: Option<PathBuf>) -> Result<()> {
    let config_path = get_config_path(custom_path);

    // Create parent directories if they don't exist
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    // Serialize and save config
    let content = serde_json::to_string_pretty(config).context("Failed to serialize config")?;

    let mut file = fs::File::create(&config_path)
        .with_context(|| format!("Failed to create config file: {}", config_path.display()))?;

    file.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

    Ok(())
}
