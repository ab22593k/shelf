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
        let prompt = format!(
            "Generate a concise and descriptive commit message for the following git diff:\n\n{}",
            diff
        );

        let chat_request = ChatRequest::default()
            .with_system("You are a helpful assistant that generates git commit messages.")
            .append_message(ChatMessage::user(&prompt));

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
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn generate_commit_message(&self, diff: &str) -> Result<String> {
        // TODO: Implement Anthropic Claude API integration
        Err(anyhow!("Anthropic provider not yet implemented"))
    }

    fn name(&self) -> &'static str {
        "anthropic"
    }
}

pub struct GeminiProvider {
    api_key: String,
}

impl GeminiProvider {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
        }
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    async fn generate_commit_message(&self, diff: &str) -> Result<String> {
        // TODO: Implement Google Gemini API integration
        Err(anyhow!("Gemini provider not yet implemented"))
    }

    fn name(&self) -> &'static str {
        "gemini"
    }
}

pub struct OllamaProvider {
    host: String,
    model: String,
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            host: "http://localhost:11434".to_string(),
            model: "deepseek-coder-v2".to_string(),
        }
    }

    pub fn with_config(host: Option<String>, model: Option<String>) -> Self {
        Self {
            host: host.unwrap_or_else(|| "http://localhost:11434".to_string()),
            model: model.unwrap_or_else(|| "deepseek-coder-v2".to_string()),
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn generate_commit_message(&self, diff: &str) -> Result<String> {
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/api/generate", self.host))
            .json(&serde_json::json!({
                "model": self.model,
                "prompt": format!(
                    "You are an expert git commit message generator. Given a git diff, create a single, concise commit message following these rules:

                      - Use imperative mood (e.g 'Add' not 'Adds')
                      - Start with a capital letter
                      - No period at the end
                      - Maximum 72 characters
                      - Focus on what changed and why
                      - Highlight scope of changes (e.g. which components)
                      - Be specific but concise
                      - One line only

                      Format your response as a single line of text with no prefix or explanation.

                      Here is the diff to analyze:\n{}",
                    diff
                ),
                "stream": false
            }))
            .send()
            .await?;

        let response = response.json::<serde_json::Value>().await?;
        Ok(response["response"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid response from Ollama"))?
            .trim()
            .to_string())
    }

    fn name(&self) -> &'static str {
        "ollama"
    }
}
