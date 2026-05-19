use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SceneSummary {
    pub start: f64,
    pub end: f64,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualitySummary {
    pub resolution_quality: Option<String>,
    pub blur: Option<String>,
    pub distortion: Option<String>,
    pub brightness: Option<String>,
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioSummary {
    pub has_audio: Option<bool>,
    pub speech: Option<String>,
    pub music: Option<String>,
    pub noise: Option<String>,
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisResult {
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub scenes: Vec<SceneSummary>,
    pub quality: QualitySummary,
    pub audio: AudioSummary,
}

pub fn response_schema_prompt() -> &'static str {
    r#"You must output strict JSON only, with this schema:
{
  "title": "string",
  "summary": "string",
  "tags": ["string"],
  "scenes": [{"start": 0.0, "end": 0.0, "description": "string", "tags": ["string"]}],
  "quality": {},
  "audio": {}
}
No markdown fences. No additional commentary."#
}

pub fn clean_model_output(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return trimmed[start..=end].to_string();
        }
    }
    trimmed.to_string()
}
