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
        self.post_chat(body)
    }

    pub fn chat_multimodal(&self, system_prompt: &str, user_text: &str, image_paths: &[String], audio_paths: &[String], temperature: f32) -> Result<String> {
        let mut content = user_text.to_string();
        if !image_paths.is_empty() {
            content.push_str("\n\nPrepared image files:\n");
            for path in image_paths { content.push_str("- "); content.push_str(path); content.push('\n'); }
        }
        if !audio_paths.is_empty() {
            content.push_str("\nPrepared mono wav audio files:\n");
            for path in audio_paths { content.push_str("- "); content.push_str(path); content.push('\n'); }
        }
        self.chat(vec![
            ChatMessage { role: "system".to_string(), content: system_prompt.to_string() },
            ChatMessage { role: "user".to_string(), content },
        ], temperature)
    }

    fn post_chat(&self, body: serde_json::Value) -> Result<String> {
        let value: serde_json::Value = self.http.post(&self.endpoint).json(&body).send()?.error_for_status()?.json()?;
        let content = value["choices"][0]["message"]["content"].as_str().unwrap_or_default().to_string();
        Ok(content)
    }
}
