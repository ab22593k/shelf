#![allow(unused)]

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod config;
pub mod providers;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn generate_commit_message(&self, diff: &str) -> Result<String>;
    fn name(&self) -> &'static str;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitAIConfig {
    pub provider: String,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub ollama_host: Option<String>,
    pub ollama_model: Option<String>,
    pub assistant_thread_id: Option<String>,
    pub project_context: Option<String>,
}

impl GitAIConfig {
    pub async fn load() -> Result<Self> {
        config::load_config(None)
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "provider" => self.provider = value.to_string(),
            "openai_api_key" => self.openai_api_key = Some(value.to_string()),
            "anthropic_api_key" => self.anthropic_api_key = Some(value.to_string()),
            "gemini_api_key" => self.gemini_api_key = Some(value.to_string()),
            "ollama_host" => self.ollama_host = Some(value.to_string()),
            "ollama_model" => self.ollama_model = Some(value.to_string()),
            "assistant_thread_id" => self.assistant_thread_id = Some(value.to_string()),
            "project_context" => self.project_context = Some(value.to_string()),
            _ => return Err(anyhow::anyhow!("Unknown config key: {}", key)),
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "provider" => Some(self.provider.clone()),
            "openai_api_key" => self.openai_api_key.clone(),
            "anthropic_api_key" => self.anthropic_api_key.clone(),
            "gemini_api_key" => self.gemini_api_key.clone(),
            "ollama_host" => self.ollama_host.clone(),
            "ollama_model" => self.ollama_model.clone(),
            "assistant_thread_id" => self.assistant_thread_id.clone(),
            "project_context" => self.project_context.clone(),
            _ => None,
        }
    }

    pub async fn save(&self) -> Result<()> {
        config::save_config(self, None)
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
        if let Some(host) = &self.ollama_host {
            items.push(("ollama_host", host.clone()));
        }
        if let Some(model) = &self.ollama_model {
            items.push(("ollama_model", model.clone()));
        }
        if let Some(id) = &self.assistant_thread_id {
            items.push(("assistant_thread_id", id.clone()));
        }
        if let Some(ctx) = &self.project_context {
            items.push(("project_context", ctx.clone()));
        }

        items
    }
}

impl Default for GitAIConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            openai_api_key: None,
            anthropic_api_key: None,
            gemini_api_key: None,
            ollama_host: Some("http://localhost:11434".to_string()),
            ollama_model: Some("qwen2.5-coder".to_string()),
            assistant_thread_id: None,
            project_context: None,
        }
    }
}

pub struct GitAI {
    config: GitAIConfig,
    provider: Box<dyn Provider>,
}

impl GitAI {
    pub async fn new(config_path: Option<PathBuf>) -> Result<Self> {
        let config = config::load_config(config_path)?;
        let provider = providers::create_provider(&config)?;

        Ok(Self { config, provider })
    }

    pub async fn generate_commit_message(&self, diff: &str) -> Result<String> {
        self.provider.generate_commit_message(diff).await
    }
}
