pub mod http;
pub mod prompt;
pub mod providers;
pub mod utils;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use prompt::PromptKind;
use providers::{OLLAMA_HOST, OLLAMA_MODEL};
use serde::{Deserialize, Serialize};

use crate::config::ConfigOp;

#[async_trait]
pub trait Provider: Send + Sync {
    /// Generate a response from the AI model
    async fn generate_assistant_message(&self, prompt: PromptKind, diff: &str) -> Result<String>;

    /// Format the user prompt with the diff
    fn user_prompt(&self, prompt: &str, diff: &str) -> String {
        format!("{}\n{}", prompt, diff)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

// Custom Debug implementation to avoid accidentally logging API keys
impl std::fmt::Display for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Serde implementations
impl Serialize for ApiKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ApiKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(ApiKey::new)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AIConfig {
    pub provider: String,
    pub openai_api_key: Option<ApiKey>,
    pub anthropic_api_key: Option<ApiKey>,
    pub gemini_api_key: Option<ApiKey>,
    pub groq_api_key: Option<ApiKey>,
    pub xai_api_key: Option<ApiKey>,
    pub ollama_host: Option<String>,
    pub ollama_model: Option<String>,
}

/// This is defaul implementation for gitaiconfig
impl Default for AIConfig {
    fn default() -> Self {
        Self {
            provider: "xai".to_string(),
            openai_api_key: None,
            anthropic_api_key: None,
            gemini_api_key: None,
            groq_api_key: None,
            xai_api_key: None,
            ollama_host: Some(OLLAMA_HOST.to_string()),
            ollama_model: Some(OLLAMA_MODEL.to_string()),
        }
    }
}

impl AIConfig {
    pub async fn load() -> Result<Self> {
        ConfigOp::load_config()
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "provider" => self.provider = value.to_string(),
            "openai_api_key" => self.openai_api_key = Some(ApiKey::new(value)),
            "anthropic_api_key" => self.anthropic_api_key = Some(ApiKey::new(value)),
            "gemini_api_key" => self.gemini_api_key = Some(ApiKey::new(value)),
            "groq_api_key" => self.groq_api_key = Some(ApiKey::new(value)),
            "xai_api_key" => self.xai_api_key = Some(ApiKey::new(value)),
            "ollama_host" => self.ollama_host = Some(value.to_string()),
            "ollama_model" => self.ollama_model = Some(value.to_string()),
            _ => return Err(anyhow!("Unknown config key: {}", key)),
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "provider" => Some(self.provider.clone()),
            "openai_api_key" => self.openai_api_key.as_ref().map(|k| k.as_str().to_string()),
            "anthropic_api_key" => self
                .anthropic_api_key
                .as_ref()
                .map(|k| k.as_str().to_string()),
            "gemini_api_key" => self.gemini_api_key.as_ref().map(|k| k.as_str().to_string()),
            "groq_api_key" => self.groq_api_key.as_ref().map(|k| k.as_str().to_string()),
            "xai_api_key" => self.xai_api_key.as_ref().map(|k| k.as_str().to_string()),
            "ollama_host" => self.ollama_host.clone(),
            "ollama_model" => self.ollama_model.clone(),
            _ => None,
        }
    }

    pub async fn save(&self) -> Result<()> {
        ConfigOp::save_config(self)
    }

    pub fn list(&self) -> Vec<(&str, String)> {
        let mut items = vec![("provider", self.provider.clone())];

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
        if let Some(model) = &self.ollama_model {
            items.push(("ollama_model", model.clone()));
        }

        items
    }
}
