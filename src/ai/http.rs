use anyhow::{anyhow, Result};
use reqwest::header::HeaderMap;

#[derive(Clone)]
pub struct HttpProvider {
    pub host: String,
    pub model: String,
    pub headers: HeaderMap,
}

#[derive(PartialEq, Debug)]
pub enum ProviderKind {
    Xai,
    Ollama,
}

impl HttpProvider {
    pub async fn make_request(
        &self,
        provider: ProviderKind,
        system_prompt: &str,
        user_prompt: &str,
        temperature: f32,
        top_p: f32,
    ) -> Result<String> {
        let client = if provider == ProviderKind::Xai {
            reqwest::Client::builder()
                .default_headers(self.headers.clone())
                .build()
                .unwrap()
        } else {
            reqwest::Client::builder().build().unwrap()
        };

        let request_body = match provider {
            ProviderKind::Xai => serde_json::json!({
                "messages": [
                    {
                        "role": "system",
                        "content": system_prompt,
                    },
                    {
                        "role": "user",
                        "content": user_prompt,
                    }
                ],
                "model": self.model,
                "stream": false,
                "temperature": temperature,
                "top_p": top_p
            }),
            ProviderKind::Ollama => serde_json::json!({
                "model": self.model,
                "system": system_prompt,
                "prompt": user_prompt,
                "stream": false,
                "temperature": temperature,
                "top_p": top_p
            }),
        };

        let chat_request = client.post(&self.host).json(&request_body).send().await?;
        let response_text = chat_request.text().await?;
        let response_json = serde_json::from_str::<serde_json::Value>(&response_text)?;

        let content = match provider {
            ProviderKind::Xai => response_json["choices"][0]["message"]["content"].clone(),
            ProviderKind::Ollama => response_json["response"].clone(),
        };
        println!("{}", content);
        Ok(content
            .as_str()
            .ok_or_else(|| anyhow!("Invalid response from `{:?}` provider", provider))?
            .trim()
            .to_string())
    }
}