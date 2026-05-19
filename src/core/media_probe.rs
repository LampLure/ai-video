use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProbeInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

pub fn probe_video(path: &str) -> Result<ProbeInfo> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height,r_frame_rate:format=duration",
            "-of", "json",
            path,
        ])
        .output();

    let Ok(output) = output else {
        return Ok(ProbeInfo::default());
    };
    if !output.status.success() {
        return Ok(ProbeInfo::default());
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let duration = value["format"]["duration"]
        .as_str()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or_default();
    let stream = value["streams"].as_array().and_then(|v| v.first()).cloned().unwrap_or_default();
    let width = stream["width"].as_u64().unwrap_or_default() as u32;
    let height = stream["height"].as_u64().unwrap_or_default() as u32;
    let fps = parse_rate(stream["r_frame_rate"].as_str().unwrap_or("0/1"));

    Ok(ProbeInfo { duration, width, height, fps })
}

fn parse_rate(rate: &str) -> f64 {
    let mut parts = rate.split('/');
    let n = parts.next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let d = parts.next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0);
    if d == 0.0 { 0.0 } else { n / d }
}
