#![allow(unused)]

use thiserror::Error;

// Define a custom error type
#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("API error (status {status}): {message}")]
    ApiKey { status: u16, message: String },
    #[error("Request error: {source}")]
    Request {
        #[from]
        source: reqwest::Error,
    },
    #[error("Response error: {source}")]
    Response {
        #[from]
        source: serde_json::Error,
    },
    #[error("Chat execution failed for model '{model}': {source}")]
    ChatExecution {
        model: String,
        #[source]
        source: genai::Error,
    },
    #[error("Missing content in response")]
    MissingContent,
}
