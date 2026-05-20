use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::Duration;

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
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { endpoint: endpoint.into(), model_name: model_name.into(), http }
    }

    pub fn chat(&self, messages: Vec<ChatMessage>, temperature: f32) -> Result<String> {
        let body = json!({
            "model": self.model_name,
            "messages": messages,
            "temperature": temperature,
            "stream": false
        });
        self.post_chat(body)
    }

    pub fn chat_streaming(&self, messages: Vec<ChatMessage>, temperature: f32) -> Result<String> {
        let body = json!({
            "model": self.model_name,
            "messages": messages,
            "temperature": temperature,
            "stream": true
        });
        self.post_chat_streaming(body)
    }

    pub fn chat_multimodal(&self, system_prompt: &str, user_text: &str, image_paths: &[String], audio_paths: &[String], temperature: f32) -> Result<String> {
        let mut content = Vec::new();
        if !system_prompt.trim().is_empty() {
            content.push(json!({ "type": "text", "text": system_prompt }));
        }
        content.push(json!({ "type": "text", "text": user_text }));
        for path in image_paths {
            let data = std::fs::read(path).with_context(|| format!("failed to read image for AI: {path}"))?;
            let b64 = general_purpose::STANDARD.encode(data);
            content.push(json!({
                "type": "image_url",
                "image_url": { "url": format!("data:image/jpeg;base64,{b64}") }
            }));
        }
        for path in audio_paths {
            let data = std::fs::read(path).with_context(|| format!("failed to read audio for AI: {path}"))?;
            let b64 = general_purpose::STANDARD.encode(data);
            content.push(json!({
                "type": "input_audio",
                "input_audio": { "data": b64, "format": "wav" }
            }));
        }
        let body = json!({
            "model": self.model_name,
            "messages": [{ "role": "user", "content": content }],
            "temperature": temperature,
            "max_tokens": 1600,
            "stream": true
        });
        self.post_chat_streaming(body)
    }

    fn post_chat(&self, body: Value) -> Result<String> {
        let value: Value = self.http.post(&self.endpoint).json(&body).send()?.error_for_status()?.json()?;
        let content = value["choices"][0]["message"]["content"].as_str().unwrap_or_default().to_string();
        Ok(content)
    }

    fn post_chat_streaming(&self, body: Value) -> Result<String> {
        let response = self.http.post(&self.endpoint).json(&body).send()?.error_for_status()?;
        let reader = BufReader::new(response);
        let mut out = String::new();
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() || !line.starts_with("data:") { continue; }
            let data = line.trim_start_matches("data:").trim();
            if data == "[DONE]" { break; }
            let Ok(value) = serde_json::from_str::<Value>(data) else { continue; };
            if let Some(s) = value.pointer("/choices/0/delta/content").and_then(Value::as_str) {
                out.push_str(s);
            } else if let Some(s) = value.pointer("/choices/0/message/content").and_then(Value::as_str) {
                out.push_str(s);
            }
        }
        if out.trim().is_empty() {
            anyhow::bail!("AI returned empty response");
        }
        Ok(out)
    }
}
