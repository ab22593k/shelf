#![allow(unused)]

pub mod error;
pub mod prompts;
pub mod provider;
pub mod utils;

use anyhow::Result;
use async_trait::async_trait;
use prompts::PromptKind;
use serde::{Deserialize, Serialize};

/// Trait for AI providers.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Generate a response from the AI model.
    async fn generate_assistant_message(&self, prompt: PromptKind, diff: &str) -> Result<String>;

    /// Format the user prompt with the diff.
    fn format_prompt(&self, prompt: &str, diff: &str) -> String {
        format!("{}\n{}", prompt, diff)
    }
}

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
