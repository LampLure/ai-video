use crate::core::cache_manager::{extract_audio_segment, extract_frames};
use crate::core::settings::AppSettings;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRequest {
    RequestSegment {
        video_path: String,
        video_hash: String,
        frame_times: Vec<f64>,
        audio_centers: Vec<f64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentRequestLimits {
    pub duration: f64,
    pub max_images: usize,
    pub max_audio_segments: usize,
    pub audio_clip_seconds: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentResponse {
    Ok { frames: Vec<String>, audio: Vec<String> },
    Error { code: String, message: String },
}

impl SegmentRequestLimits {
    pub fn from_settings(duration: f64, settings: &AppSettings) -> Self {
        Self {
            duration,
            max_images: settings.max_images,
            max_audio_segments: settings.max_audio_segments,
            audio_clip_seconds: settings.audio_clip_seconds,
        }
    }
}

pub fn handle_agent_request(req: AgentRequest, limits: SegmentRequestLimits, settings: &AppSettings) -> Result<AgentResponse> {
    match req {
        AgentRequest::RequestSegment { video_path, video_hash, frame_times, audio_centers } => {
            if frame_times.len() > limits.max_images {
                return Ok(AgentResponse::Error { code: "too_many_images".into(), message: format!("requested {} images, limit is {}", frame_times.len(), limits.max_images) });
            }
            if audio_centers.len() > limits.max_audio_segments {
                return Ok(AgentResponse::Error { code: "too_many_audio_segments".into(), message: format!("requested {} audio segments, limit is {}", audio_centers.len(), limits.max_audio_segments) });
            }
            if frame_times.iter().chain(audio_centers.iter()).any(|v| *v < 0.0 || *v > limits.duration) {
                return Ok(AgentResponse::Error { code: "timestamp_out_of_range".into(), message: format!("timestamp must be between 0 and {:.3}", limits.duration) });
            }
            if audio_centers.len() == 1 && limits.duration > limits.audio_clip_seconds as f64 * 2.0 {
                return Ok(AgentResponse::Error {
                    code: "audio_request_too_broad".into(),
                    message: format!(
                        "audio must use centered clips. Request more audio_centers for wider coverage. Each clip is {:.1}s and at most {} clips are allowed.",
                        limits.audio_clip_seconds,
                        limits.max_audio_segments
                    ),
                });
            }
            let frames = extract_frames(&video_path, &video_hash, &frame_times, settings.image_pixel_limit)?;
            let mut audio = Vec::new();
            for center in audio_centers {
                audio.push(extract_audio_segment(&video_path, &video_hash, center, settings.audio_clip_seconds, settings.audio_sample_rate)?);
            }
            Ok(AgentResponse::Ok { frames, audio })
        }
    }
}

pub fn build_initial_constraints_message(duration: f64, settings: &AppSettings) -> String {
    format!(
        "Video total duration: {duration:.3}s. Limits: max_images={}, max_audio_segments={}, audio_clip_seconds={}, image_pixel_limit={}, max_context_tokens={}. Audio must be requested as centered clips through audio_centers. Each clip is audio_clip_seconds seconds long and at most max_audio_segments clips are allowed.",
        settings.max_images,
        settings.max_audio_segments,
        settings.audio_clip_seconds,
        settings.image_pixel_limit,
        settings.max_context_tokens
    )
}
