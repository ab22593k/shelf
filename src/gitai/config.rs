use super::GitAIConfig;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;

const DEFAULT_CONFIG_NAME: &str = "gitai.json";

pub fn load_config(config_path: Option<PathBuf>) -> Result<GitAIConfig> {
    let config_path = config_path.unwrap_or_else(|| {
        let xdg_config = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").expect("HOME environment variable not set");
                PathBuf::from(home).join(".config")
            });
        xdg_config.join("shelf").join(DEFAULT_CONFIG_NAME)
    });

    if config_path.exists() {
        let content = fs::read_to_string(config_path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(GitAIConfig::default())
    }
}

pub fn save_config(config: &GitAIConfig, config_path: Option<PathBuf>) -> Result<()> {
    let config_path = config_path.unwrap_or_else(|| {
        let xdg_config = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").expect("HOME environment variable not set");
                PathBuf::from(home).join(".config")
            });
        xdg_config.join("shelf").join(DEFAULT_CONFIG_NAME)
    });

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(config)?;
    fs::write(config_path, content)?;
    Ok(())
}
