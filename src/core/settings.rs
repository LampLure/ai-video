use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheSwitchPolicy {
    Keep,
    ClearOnFolderChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub max_images: usize,
    pub max_audio_segments: usize,
    pub audio_clip_seconds: f32,
    pub image_pixel_limit: u32,
    pub audio_sample_rate: u32,
    pub max_context_tokens: usize,
    pub cache_size_limit_mb: u64,
    pub cache_switch_policy: CacheSwitchPolicy,
    pub debug_mode: bool,
    pub llama_cpp_endpoint: String,
    pub model_name: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            max_images: 10,
            max_audio_segments: 10,
            audio_clip_seconds: 5.0,
            image_pixel_limit: 24_000,
            audio_sample_rate: 16_000,
            max_context_tokens: 8192,
            cache_size_limit_mb: 4096,
            cache_switch_policy: CacheSwitchPolicy::Keep,
            debug_mode: false,
            llama_cpp_endpoint: "http://127.0.0.1:7080/v1/chat/completions".to_string(),
            model_name: "local-multimodal".to_string(),
        }
    }
}
