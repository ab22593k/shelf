#![allow(unused)]

use super::{GitAIConfig, Provider};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use colored::Colorize;
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
    "You are a Git Commit Message Generator Assistant. Your role is to help developers create clear, concise, and meaningful commit messages following best practices.

    INPUT EXPECTATIONS:
    - You will receive git diff output or a description of changes made to the code
    - The changes might include multiple files and various types of modifications

    OUTPUT REQUIREMENTS:
    1. Format: Follow the Conventional Commits specification:
       <type>[optional scope]: <description> <imoji>

       [optional body]

       [optional footer(s)]

    2. Types to use:
       - feat: New feature
       - fix: Bug fix
       - docs: Documentation changes
       - style: Code style changes (formatting, etc.)
       - refactor: Code changes that neither fix bugs nor add features
       - perf: Performance improvements
       - test: Adding or modifying tests
       - chore: Maintenance tasks

    3. Description Guidelines:
       - Use imperative mood ('add' not 'added' or 'adds')
       - Keep first line under 50 characters
       - Don't capitalize first letter
       - No period at the end
       - Be specific but concise

    4. Add Body only:
       - Explain breaking changes
       - Describe complex changes
       - Explain the motivation for changes
       - Document side effects

    EXAMPLE RESPONSES:

    For simple changes:
    feat: add user authentication endpoint

    For complex changes:
    feat(auth): implement OAuth2 social login

    This change adds support for social login via OAuth2 protocol,
    currently supporting Google and GitHub providers.

    BREAKING CHANGE: Authentication header format has changed

    For bug fixes:
    fix(api): prevent race condition in payment processing

    Special Instructions:
    1. If changes affect multiple areas, focus on the primary change
    2. If breaking changes exist, always include them in the footer
    3. Include relevant ticket/issue numbers if provided
    4. Use scope to indicate the component being modified

    Remember: A good commit message should complete this sentence:
    'If applied, this commit will... <your commit message>'";

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
                "system": PROMPT,
                "prompt": format!("git diff output:\n{}", diff),
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
