#![allow(unused)]

use super::{GitAIConfig, Provider};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use genai::{
    self,
    chat::{ChatMessage, ChatRequest},
    resolver::{AuthData, AuthResolver},
    Client, ModelIden,
};
use reqwest;
use serde_json;

pub const OLLAMA_HOST: &str = "http://localhost:11434";
pub const OLLAMA_MODEL: &str = "qwen2.5-coder";

pub const PROMPT: &str =
    "You are a Git commit message generator. create a clear and concise commit message.
 ** Rules:
 * Use the imperative mood: Write the message as a command, e.g., 'Fix bug' instead of 'Fixed bug.'
 * Keep it concise: brief and informative message on batch of similar changes.
 * Use a consistent style: Follow a consistent formatting style.";

pub fn create_provider(config: &GitAIConfig) -> Result<Box<dyn Provider>> {
    let provider: Box<dyn Provider> = match config.provider.as_str() {
        "openai" => {
            let api_key = config
                .openai_api_key
                .as_ref()
                .ok_or_else(|| anyhow!("OpenAI API key not configured"))?;
            Box::new(OpenAIProvider::new(api_key))
        }
        "anthropic" => {
            let api_key = config
                .anthropic_api_key
                .as_ref()
                .ok_or_else(|| anyhow!("Anthropic API key not configured"))?;
            Box::new(AnthropicProvider::new(api_key))
        }
        "gemini" => {
            let api_key = config
                .gemini_api_key
                .as_ref()
                .ok_or_else(|| anyhow!("Google Gemini API key not configured"))?;
            Box::new(GeminiProvider::new(api_key))
        }
        "ollama" => Box::new(OllamaProvider::with_config(
            config.ollama_host.clone(),
            config.ollama_model.clone(),
        )),
        _ => return Err(anyhow!("Unsupported provider: {}", config.provider)),
    };
    Ok(provider)
}

pub struct OpenAIProvider {
    client: Client,
    model: String,
}

impl OpenAIProvider {
    pub fn new(api_key: &str) -> Self {
        let api_key = api_key.to_string();
        let auth_resolver = AuthResolver::from_resolver_fn(
            move |_model_iden: ModelIden| -> Result<Option<AuthData>, genai::resolver::Error> {
                Ok(Some(AuthData::from_single(api_key.clone())))
            },
        );

        Self {
            client: Client::builder().with_auth_resolver(auth_resolver).build(),
            model: "gpt-3.5-turbo".to_string(),
        }
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    async fn generate_commit_message(&self, diff: &str) -> Result<String> {
        let prompt = format!("{}\n\n{}", PROMPT, diff);

        let chat_request = ChatRequest::default().append_message(ChatMessage::user(&prompt));

        let response = self
            .client
            .exec_chat(&self.model, chat_request, None)
            .await?;

        Ok(response
            .content
            .unwrap()
            .text_as_str()
            .unwrap()
            .trim()
            .to_string())
    }

    fn name(&self) -> &'static str {
        "openai"
    }
}

pub struct AnthropicProvider {
    client: Client,
    model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: &str) -> Self {
        let api_key = api_key.to_string();
        let auth_resolver = AuthResolver::from_resolver_fn(
            move |_model_iden: ModelIden| -> Result<Option<AuthData>, genai::resolver::Error> {
                Ok(Some(AuthData::from_single(api_key.clone())))
            },
        );

        Self {
            client: Client::builder().with_auth_resolver(auth_resolver).build(),
            model: "claude-3.5-sonnet".to_string(),
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn generate_commit_message(&self, diff: &str) -> Result<String> {
        let prompt = format!("{}\n\n{}", PROMPT, diff);

        let chat_request = ChatRequest::default().append_message(ChatMessage::user(&prompt));

        let response = self
            .client
            .exec_chat(&self.model, chat_request, None)
            .await?;

        Ok(response
            .content
            .unwrap()
            .text_as_str()
            .unwrap()
            .trim()
            .to_string())
    }

    fn name(&self) -> &'static str {
        "anthropic"
    }
}

pub struct GeminiProvider {
    client: Client,
    model: String,
}

impl GeminiProvider {
    pub fn new(api_key: &str) -> Self {
        let api_key = api_key.to_string();
        let auth_resolver = AuthResolver::from_resolver_fn(
            move |_model_iden: ModelIden| -> Result<Option<AuthData>, genai::resolver::Error> {
                Ok(Some(AuthData::from_single(api_key.clone())))
            },
        );

        Self {
            client: Client::builder().with_auth_resolver(auth_resolver).build(),
            model: "gemini-1.5-flash".to_string(),
        }
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    async fn generate_commit_message(&self, diff: &str) -> Result<String> {
        let prompt = format!("{}\n\n{}", PROMPT, diff);

        let chat_request = ChatRequest::default().append_message(ChatMessage::user(&prompt));

        let response = self
            .client
            .exec_chat(&self.model, chat_request, None)
            .await?;

        Ok(response
            .content
            .unwrap()
            .text_as_str()
            .unwrap()
            .trim()
            .to_string())
    }

    fn name(&self) -> &'static str {
        "gemini"
    }
}

pub struct OllamaProvider {
    host: String,
    model: String,
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            host: OLLAMA_HOST.to_string(),
            model: OLLAMA_MODEL.to_string(),
        }
    }

    pub fn with_config(host: Option<String>, model: Option<String>) -> Self {
        Self {
            host: host.unwrap_or_else(|| OLLAMA_HOST.to_string()),
            model: model.unwrap_or_else(|| OLLAMA_MODEL.to_string()),
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn generate_commit_message(&self, diff: &str) -> Result<String> {
        let client = reqwest::Client::new();
        let request = client
            .post(format!("{}/api/generate", self.host))
            .json(&serde_json::json!({
                "model": self.model,
                "prompt": format!("{}\nProvided git diff:\n{}", PROMPT, diff),
                "stream": false
            }))
            .send()
            .await?;

        let response_json: serde_json::Value = serde_json::from_str(&request.text().await?)?;
        Ok(response_json["response"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid response from Ollama"))?
            .trim()
            .to_string())
    }

    fn name(&self) -> &'static str {
        "ollama"
    }
}
