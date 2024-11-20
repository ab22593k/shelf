pub mod git;
pub mod providers;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use providers::{OLLAMA_HOST, OLLAMA_MODEL};
use serde::{Deserialize, Serialize};

use crate::config::ShelfConfig;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn generate_commit_message(&self, diff: &str) -> Result<String>;

    fn prompt(&self, diff: &str) -> String {
        format!(
            "Generate a concise commit message for the following git diff output:\n{}",
            diff
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitAIConfig {
    pub provider: String,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub groq_api_key: Option<String>,
    pub ollama_host: Option<String>,
    pub ollama_model: Option<String>,
}

impl GitAIConfig {
    pub async fn load() -> Result<Self> {
        ShelfConfig::load_config()
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "provider" => self.provider = value.to_string(),
            "openai_api_key" => self.openai_api_key = Some(value.to_string()),
            "anthropic_api_key" => self.anthropic_api_key = Some(value.to_string()),
            "gemini_api_key" => self.gemini_api_key = Some(value.to_string()),
            "groq_api_key" => self.groq_api_key = Some(value.to_string()),
            "ollama_host" => self.ollama_host = Some(value.to_string()),
            "ollama_model" => self.ollama_model = Some(value.to_string()),
            _ => return Err(anyhow!("Unknown config key: {}", key)),
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "provider" => Some(self.provider.clone()),
            "openai_api_key" => self.openai_api_key.clone(),
            "anthropic_api_key" => self.anthropic_api_key.clone(),
            "gemini_api_key" => self.gemini_api_key.clone(),
            "groq_api_key" => self.groq_api_key.clone(),
            "ollama_host" => self.ollama_host.clone(),
            "ollama_model" => self.ollama_model.clone(),
            _ => None,
        }
    }

    pub async fn save(&self) -> Result<()> {
        ShelfConfig::save_config(self)
    }

    pub fn list(&self) -> Vec<(&str, String)> {
        let mut items = vec![("provider", self.provider.clone())];

        if let Some(key) = &self.openai_api_key {
            items.push(("openai_api_key", key.clone()));
        }
        if let Some(key) = &self.anthropic_api_key {
            items.push(("anthropic_api_key", key.clone()));
        }
        if let Some(key) = &self.gemini_api_key {
            items.push(("gemini_api_key", key.clone()));
        }
        if let Some(key) = &self.groq_api_key {
            items.push(("groq_api_key", key.clone()));
        }
        if let Some(host) = &self.ollama_host {
            items.push(("ollama_host", host.clone()));
        }
        if let Some(model) = &self.ollama_model {
            items.push(("ollama_model", model.clone()));
        }

        items
    }
}

impl Default for GitAIConfig {
    fn default() -> Self {
        Self {
            provider: "groq".to_string(),
            openai_api_key: None,
            anthropic_api_key: None,
            gemini_api_key: None,
            groq_api_key: None,
            ollama_host: Some(OLLAMA_HOST.to_string()),
            ollama_model: Some(OLLAMA_MODEL.to_string()),
        }
    }
}
