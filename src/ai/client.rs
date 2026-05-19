use anyhow::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct AiClient {
    endpoint: String,
    model_name: String,
    http: Client,
}

impl AiClient {
    pub fn new(endpoint: impl Into<String>, model_name: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into(), model_name: model_name.into(), http: Client::new() }
    }

    pub fn chat(&self, messages: Vec<ChatMessage>, temperature: f32) -> Result<String> {
        let body = serde_json::json!({
            "model": self.model_name,
            "messages": messages,
            "temperature": temperature,
            "stream": false
        });
        let value: serde_json::Value = self.http.post(&self.endpoint).json(&body).send()?.error_for_status()?.json()?;
        let content = value["choices"][0]["message"]["content"].as_str().unwrap_or_default().to_string();
        Ok(content)
    }
}
