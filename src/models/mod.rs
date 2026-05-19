use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelConfig {
    pub name: String,
    pub endpoint: String,
    pub launch_script: Option<String>,
    pub supports_images: bool,
    pub supports_audio: bool,
}

impl Default for LocalModelConfig {
    fn default() -> Self {
        Self {
            name: "local-llamacpp".to_string(),
            endpoint: "http://127.0.0.1:7080/v1/chat/completions".to_string(),
            launch_script: None,
            supports_images: true,
            supports_audio: false,
        }
    }
}
