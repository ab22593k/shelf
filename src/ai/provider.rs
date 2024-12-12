use crate::configure::AIProviderConfig;

use super::{error::ProviderError, prompts::PromptKind, ApiKey, Provider};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use genai::{
    self,
    chat::{ChatMessage, ChatOptions, ChatRequest},
    resolver::{AuthData, AuthResolver},
    Client, ModelIden,
};

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
        user_prompt: &str,
        diff: &str,
    ) -> ChatRequest {
        ChatRequest::default()
            .append_message(ChatMessage::system(system_prompt))
            .append_message(ChatMessage::user(format!("{}\n{}", user_prompt, diff)))
    }

    fn set_chat_options(&self, temperature: f64, top_p: f64) -> ChatOptions {
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
        "xai" => create_api_provider::<XAIProvider>(config.xai_api_key.as_ref()),
        "ollama" => create_api_provider::<OllamaProvider>(Some(&ApiKey("".to_string()))),
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
                let options = self.set_chat_options(0.2, 0.95);

                let response = self
                    .get_client()
                    .exec_chat(self.get_model(), chat_request, Some(&options))
                    .await
                    .map_err(|e| ProviderError::ChatExecution {
                        model: self.get_model().to_string(),
                        source: e,
                    })?;

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

impl_provider!(GeminiProvider, "gemini-exp-1206");
impl_provider!(GroqProvider, "llama-3.3-70b-versatile");
impl_provider!(XAIProvider, "grok-beta");
impl_provider!(OpenAIProvider, "gpt-3.5-turbo");
impl_provider!(AnthropicProvider, "claude-3.5-sonnet");
impl_provider!(OllamaProvider, "qwen2.5-coder");
