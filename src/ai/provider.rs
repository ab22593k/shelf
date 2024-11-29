use crate::config::AIProviderConfig;

use super::{
    http::{HttpProvider, ProviderKind},
    prompt::PromptKind,
    Provider,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use genai::{
    self,
    chat::{ChatMessage, ChatOptions, ChatRequest},
    resolver::{AuthData, AuthResolver},
    Client, ModelIden,
};
use serde::{Deserialize, Serialize};

pub const XAI_HOST: &str = "https://api.x.ai/v1/chat/completions";
pub const OLLAMA_HOST: &str = "http://localhost:11434";
pub const OLLAMA_MODEL: &str = "qwen2.5-coder";

/// Wrapper for API keys.  Provides custom Debug and Serde implementations to avoid logging sensitive data.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApiKey(String);

impl ApiKey {
    /// Create a new ApiKey.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Return the API key as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the ApiKey and return the inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

/// Custom Debug implementation to avoid accidentally logging API keys.
impl std::fmt::Display for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Serde serialization implementation for ApiKey.
impl Serialize for ApiKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// Serde deserialization implementation for ApiKey.
impl<'de> Deserialize<'de> for ApiKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(ApiKey::new)
    }
}

// Base trait for common provider functionality
trait BaseProvider {
    fn get_client(&self) -> &Client;
    fn get_model(&self) -> &str;

    fn create_auth_resolver(api_key: String) -> AuthResolver {
        AuthResolver::from_resolver_fn(
            move |_: ModelIden| -> Result<Option<AuthData>, genai::resolver::Error> {
                Ok(Some(AuthData::from_single(api_key.clone())))
            },
        )
    }

    fn build_chat_request(
        &self,
        system_prompt: String,
        user_prompt: String,
        diff: &str,
    ) -> ChatRequest {
        ChatRequest::default()
            .append_message(ChatMessage::system(system_prompt))
            .append_message(ChatMessage::user(format!("{}\n{}", user_prompt, diff)))
    }

    fn get_chat_options(&self, temperature: f64, top_p: f64) -> ChatOptions {
        ChatOptions::default()
            .with_temperature(temperature)
            .with_top_p(top_p)
    }
}

pub fn create_provider(config: &AIProviderConfig) -> Result<Box<dyn Provider>> {
    match config.provider.as_str() {
        "openai" => create_api_provider::<OpenAIProvider>(config.openai_api_key.as_ref()),
        "anthropic" => create_api_provider::<AnthropicProvider>(config.anthropic_api_key.as_ref()),
        "gemini" => create_api_provider::<GeminiProvider>(config.gemini_api_key.as_ref()),
        "groq" => create_api_provider::<GroqProvider>(config.groq_api_key.as_ref()),
        "xai" => Ok(Box::new(XAIProvider::new(
            config
                .xai_api_key
                .as_ref()
                .ok_or_else(|| anyhow!("XAI API key not configured"))?,
        ))),
        "ollama" => Ok(Box::new(OllamaProvider::with_config(
            config.ollama_host.clone(),
            Some(config.model.clone()),
        ))),
        _ => Err(anyhow!("Unsupported provider: {}", config.provider)),
    }
}

fn create_api_provider<T>(api_key: Option<&ApiKey>) -> Result<Box<dyn Provider>>
where
    T: 'static + Provider + From<ApiKey>,
{
    api_key
        .ok_or_else(|| anyhow!("API key not configured"))
        .map(|key| Box::new(T::from(key.clone())) as Box<dyn Provider>)
}

// Provider implementations with shared functionality
macro_rules! impl_provider {
    ($provider:ident, $model:expr) => {
        pub struct $provider {
            client: Client,
            model: String,
        }

        impl From<ApiKey> for $provider {
            fn from(api_key: ApiKey) -> Self {
                let auth_resolver = Self::create_auth_resolver(api_key.to_string());
                Self {
                    client: Client::builder().with_auth_resolver(auth_resolver).build(),
                    model: $model.to_string(),
                }
            }
        }

        impl BaseProvider for $provider {
            fn get_client(&self) -> &Client {
                &self.client
            }
            fn get_model(&self) -> &str {
                &self.model
            }
        }

        #[async_trait]
        impl Provider for $provider {
            async fn generate_assistant_message(
                &self,
                prompt: PromptKind,
                diff: &str,
            ) -> Result<String> {
                let (system_prompt, user_prompt) =
                    { (prompt.get_system_prompt()?, prompt.get_user_prompt()?) };

                let chat_request = self.build_chat_request(system_prompt, user_prompt, diff);
                let options = self.get_chat_options(0.2, 0.95);

                let response = self
                    .get_client()
                    .exec_chat(self.get_model(), chat_request, Some(&options))
                    .await
                    .map_err(|e| anyhow!("Chat execution failed: {:?}", e))?; // TODO: Better error handling

                Ok(response
                    .content
                    .unwrap()
                    .text_as_str()
                    .unwrap()
                    .trim()
                    .to_string())
            }
        }
    };
}

impl_provider!(OpenAIProvider, "gpt-3.5-turbo");
impl_provider!(AnthropicProvider, "claude-3.5-sonnet");
impl_provider!(GeminiProvider, "gemini-1.5-pro");
impl_provider!(GroqProvider, "mixtral-8x7b-32768");

// HTTP-based providers
pub struct XAIProvider {
    provider: HttpProvider,
}

impl XAIProvider {
    fn new(api_key: &ApiKey) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert(
            "Authorization",
            format!("Bearer {}", api_key).parse().unwrap(),
        );

        Self {
            provider: HttpProvider {
                host: XAI_HOST.to_string(),
                model: "grok-beta".to_string(),
                headers,
            },
        }
    }
}

#[async_trait]
impl Provider for XAIProvider {
    async fn generate_assistant_message(&self, prompt: PromptKind, diff: &str) -> Result<String> {
        self.provider
            .make_request(
                ProviderKind::Xai,
                &prompt.get_system_prompt()?,
                &self.format_prompt(&prompt.get_user_prompt()?, diff),
                0.2,
                0.95,
            )
            .await
    }
}

pub struct OllamaProvider {
    provider: HttpProvider,
}

impl OllamaProvider {
    pub fn with_config(host: Option<String>, model: Option<String>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());

        Self {
            provider: HttpProvider {
                host: host.unwrap_or_else(|| format!("{}/api/generate", OLLAMA_HOST)),
                model: model.unwrap_or_else(|| OLLAMA_MODEL.to_string()),
                headers,
            },
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn generate_assistant_message(&self, prompt: PromptKind, diff: &str) -> Result<String> {
        self.provider
            .make_request(
                ProviderKind::Ollama,
                &prompt.get_system_prompt()?,
                &self.format_prompt(&prompt.get_user_prompt()?, diff),
                0.2,
                0.95,
            )
            .await
    }
}
