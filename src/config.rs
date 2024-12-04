use crate::{
    ai::{
        git::{install_git_hook, remove_git_hook},
        provider::{ApiKey, OLLAMA_HOST},
    },
    app::AIConfigAction,
};

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};

use std::{fs, io::Write, path::PathBuf};

pub const AI_SETTINGS_FILENAME: &str = "ai.json";

/// Represents configuration operations for the application.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// Path to the configuration file.
    pub path: PathBuf,

    /// AI configuration.
    pub ai: AIProviderConfig,
}

impl Default for Config {
    fn default() -> Self {
        let path = directories::BaseDirs::new()
            .map(|base_dirs| base_dirs.config_dir().join("shelf"))
            .expect("Could not create `shelf` configuration directory");

        Self {
            path,
            ai: AIProviderConfig::default(),
        }
    }
}

impl Config {
    /// Loads the AI configuration from the settings file.
    ///
    /// If the file exists, it attempts to deserialize the contents into an `AIConfig`.
    /// If the file doesn't exist, it creates a default `AIConfig`, saves it to the file, and returns it.
    pub fn read_all(&self) -> Result<AIProviderConfig> {
        let ai_json_file = Self::default().path.join(AI_SETTINGS_FILENAME);
        if ai_json_file.exists() {
            let content = fs::read_to_string(&ai_json_file).with_context(|| {
                format!("Failed to read config file: {}", ai_json_file.display())
            })?;

            let config: AIProviderConfig = serde_json::from_str(&content).with_context(|| {
                format!("Failed to parse config file: {}", ai_json_file.display())
            })?;

            Ok(config)
        } else {
            // Create default config and save it
            let config = AIProviderConfig::default();
            Self::write_all(&config)?;
            Ok(config)
        }
    }

    /// Saves the AI configuration to the settings file.
    ///
    /// Serializes the `config` to JSON and writes it to the file specified by `create_ai_settings()`.
    /// Creates any necessary parent directories.
    pub fn write_all(config: &AIProviderConfig) -> Result<()> {
        let ai_json_file = Self::default().path.join(AI_SETTINGS_FILENAME);

        // Create parent directories if they don't exist
        if let Some(parent) = ai_json_file.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        // Serialize and save config
        let content = serde_json::to_string_pretty(config).context("Failed to serialize config")?;

        let mut file = fs::File::create(&ai_json_file)
            .with_context(|| format!("Failed to create config file: {}", ai_json_file.display()))?;

        file.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write config file: {}", ai_json_file.display()))?;

        Ok(())
    }
}

/// Configuration for AI providers.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AIProviderConfig {
    /// The name of the provider to use.
    pub provider: String,
    pub model: String,
    pub openai_api_key: Option<ApiKey>,
    pub anthropic_api_key: Option<ApiKey>,
    pub gemini_api_key: Option<ApiKey>,
    pub groq_api_key: Option<ApiKey>,
    pub xai_api_key: Option<ApiKey>,
    pub ollama_host: Option<String>,
}

/// Default configuration for AI providers. Uses XAI as default provider.
impl Default for AIProviderConfig {
    fn default() -> Self {
        Self {
            provider: "gemini".to_string(),
            model: "gemini-1.5-flash".to_string(),
            openai_api_key: None,
            anthropic_api_key: None,
            gemini_api_key: None,
            groq_api_key: None,
            xai_api_key: None,
            ollama_host: Some(OLLAMA_HOST.to_string()),
        }
    }
}

impl AIProviderConfig {
    /// Set a configuration value.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "provider" => self.provider = value.to_string(),
            "model" => self.model = value.to_string(),
            "openai_api_key" => self.openai_api_key = Some(ApiKey::new(value)),
            "anthropic_api_key" => self.anthropic_api_key = Some(ApiKey::new(value)),
            "gemini_api_key" => self.gemini_api_key = Some(ApiKey::new(value)),
            "groq_api_key" => self.groq_api_key = Some(ApiKey::new(value)),
            "xai_api_key" => self.xai_api_key = Some(ApiKey::new(value)),
            "ollama_host" => self.ollama_host = Some(value.to_string()),
            _ => return Err(anyhow!("Unknown config key: {}", key)),
        }

        Ok(())
    }

    /// Get a configuration value.
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "provider" => Some(self.provider.clone()),
            "model" => Some(self.model.clone()),
            "openai_api_key" => self.openai_api_key.as_ref().map(|k| k.as_str().to_string()),
            "anthropic_api_key" => self
                .anthropic_api_key
                .as_ref()
                .map(|k| k.as_str().to_string()),
            "gemini_api_key" => self.gemini_api_key.as_ref().map(|k| k.as_str().to_string()),
            "groq_api_key" => self.groq_api_key.as_ref().map(|k| k.as_str().to_string()),
            "xai_api_key" => self.xai_api_key.as_ref().map(|k| k.as_str().to_string()),
            "ollama_host" => self.ollama_host.clone(),
            _ => None,
        }
    }

    /// Save the AI configuration to the config file.
    pub async fn write_all(&self) -> Result<()> {
        Config::write_all(self)
    }

    /// List all configuration values.
    pub fn list(&self) -> Vec<(&str, String)> {
        let mut items = vec![
            ("provider", self.provider.clone()),
            ("model", self.model.clone()),
        ];

        if let Some(key) = &self.openai_api_key {
            items.push(("openai_api_key", key.clone().into_inner()));
        }
        if let Some(key) = &self.anthropic_api_key {
            items.push(("anthropic_api_key", key.clone().to_string()));
        }
        if let Some(key) = &self.gemini_api_key {
            items.push(("gemini_api_key", key.clone().to_string()));
        }
        if let Some(key) = &self.groq_api_key {
            items.push(("groq_api_key", key.clone().to_string()));
        }
        if let Some(key) = &self.xai_api_key {
            items.push(("xai_api_key", key.clone().to_string()));
        }
        if let Some(host) = &self.ollama_host {
            items.push(("ollama_host", host.clone()));
        }

        items
    }
}

pub async fn handle_ai_config(config: Config, action: AIConfigAction) -> Result<()> {
    let mut config = config.read_all()?;
    match action {
        AIConfigAction::Set { key, value } => {
            config.set(&key, &value)?;
            config.write_all().await?;
            println!("{} {} = {}", "Set:".green().bold(), key, value);
        }

        AIConfigAction::Get { key } => {
            if let Some(value) = config.get(&key) {
                println!("{}", value);
            } else {
                println!("{} Key not found: {}", "Error:".red().bold(), key);
            }
        }

        AIConfigAction::List => {
            println!("{}", "Configuration:".green().bold());
            config
                .list()
                .iter()
                .for_each(|(k, v)| println!("{} = {}", k, v));
        }

        AIConfigAction::Hook { install, uninstall } => match git2::Repository::open_from_env() {
            Ok(repo) => {
                let hooks_dir = repo.path().join("hooks");

                if install {
                    match install_git_hook(&hooks_dir) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("{} Failed to install hook: {}", "Error:".red().bold(), e);
                        }
                    }
                } else if uninstall {
                    match remove_git_hook(&hooks_dir) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("{} Failed to uninstall hook: {}", "Error:".red().bold(), e);
                        }
                    }
                }
            }
            Err(e) => println!(
                "{} Failed to open git repository: {}",
                "Error:".red().bold(),
                e
            ),
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_default_config_path_linux() {
        let temp_dir = tempdir().unwrap();

        // Set XDG_CONFIG_HOME to a temporary directory
        std::env::set_var("XDG_CONFIG_HOME", temp_dir.path());

        let config = Config::default();
        let expected_path = temp_dir.path().join("shelf");
        assert_eq!(config.path, expected_path);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_default_config_path_macos() {
        use std::env;

        let temp_home = tempdir().unwrap();
        env::set_var("HOME", temp_home.path());

        let config = Config::default();
        let expected_path = temp_home
            .path()
            .join("Library")
            .join("Application Support")
            .join("shelf");

        assert_eq!(config.path, expected_path);
    }

    #[test]
    fn test_config_operations() {
        let temp_dir = tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", temp_dir.path());

        let mut config = AIProviderConfig::default();
        config.set("gemini_api_key", "api_key").unwrap();

        assert_eq!(config.get("provider"), Some("gemini".to_string()));
        assert_eq!(config.get("model"), Some("gemini-1.5-flash".to_string()));

        // Test invalid key
        assert!(config.set("invalid_key", "value").is_err());
        assert_eq!(config.get("invalid_key"), None);
    }
}
