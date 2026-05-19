use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePaths {
    pub root: String,
    pub thumb: String,
    pub frames_dir: String,
    pub audio: String,
}

pub fn default_cache_root() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| PathBuf::from("cache")).join("ai-video")
}

pub fn ensure_cache_layout(root: &Path, video_hash: &str) -> Result<CachePaths> {
    let thumbs = root.join("thumbs");
    let frames = root.join("frames").join(video_hash);
    let audio_dir = root.join("audio");
    fs::create_dir_all(&thumbs)?;
    fs::create_dir_all(&frames)?;
    fs::create_dir_all(&audio_dir)?;
    Ok(CachePaths {
        root: root.to_string_lossy().to_string(),
        thumb: thumbs.join(format!("{video_hash}.jpg")).to_string_lossy().to_string(),
        frames_dir: frames.to_string_lossy().to_string(),
        audio: audio_dir.join(format!("{video_hash}.wav")).to_string_lossy().to_string(),
    })
}

pub fn extract_thumbnail(video_path: &str, video_hash: &str, at_seconds: f64) -> Result<String> {
    let root = default_cache_root();
    let paths = ensure_cache_layout(&root, video_hash)?;
    if Path::new(&paths.thumb).exists() {
        return Ok(paths.thumb);
    }
    let timestamp = format!("{:.3}", at_seconds.max(0.0));
    let status = Command::new("ffmpeg")
        .args(["-y", "-ss", &timestamp, "-i", video_path, "-frames:v", "1", "-q:v", "4", &paths.thumb])
        .status()
        .context("failed to launch ffmpeg for thumbnail extraction")?;
    if !status.success() {
        anyhow::bail!("ffmpeg thumbnail extraction failed");
    }
    Ok(paths.thumb)
}

pub fn extract_frames(video_path: &str, video_hash: &str, timestamps: &[f64], pixel_limit: u32) -> Result<Vec<String>> {
    let root = default_cache_root();
    let paths = ensure_cache_layout(&root, video_hash)?;
    let mut out = Vec::new();
    let scale = format!("scale='if(gt(iw*ih,{0}),trunc(iw*sqrt({0}/(iw*ih))/2)*2,iw)':'if(gt(iw*ih,{0}),trunc(ih*sqrt({0}/(iw*ih))/2)*2,ih)'", pixel_limit);
    for (idx, ts) in timestamps.iter().enumerate() {
        let frame = Path::new(&paths.frames_dir).join(format!("frame_{idx:03}.jpg"));
        if frame.exists() {
            out.push(frame.to_string_lossy().to_string());
            continue;
        }
        let timestamp = format!("{:.3}", ts.max(0.0));
        let status = Command::new("ffmpeg")
            .args(["-y", "-ss", &timestamp, "-i", video_path, "-frames:v", "1", "-vf", &scale, "-q:v", "5", &frame.to_string_lossy()])
            .status()?;
        if status.success() {
            out.push(frame.to_string_lossy().to_string());
        }
    }
    Ok(out)
}

pub fn extract_audio_segment(video_path: &str, video_hash: &str, center_seconds: f64, clip_seconds: f32, sample_rate: u32) -> Result<String> {
    let root = default_cache_root();
    let paths = ensure_cache_layout(&root, video_hash)?;
    let safe_center = center_seconds.max(0.0);
    let start = (safe_center - (clip_seconds as f64 / 2.0)).max(0.0);
    let audio = Path::new(&paths.audio).with_file_name(format!("{}_{}ms.wav", video_hash, (safe_center * 1000.0) as u64));
    if audio.exists() {
        return Ok(audio.to_string_lossy().to_string());
    }
    let status = Command::new("ffmpeg")
        .args([
            "-y", "-ss", &format!("{start:.3}"), "-i", video_path,
            "-t", &format!("{clip_seconds:.3}"), "-ac", "1", "-ar", &sample_rate.to_string(),
            "-vn", &audio.to_string_lossy(),
        ])
        .status()?;
    if !status.success() {
        anyhow::bail!("ffmpeg audio extraction failed");
    }
    Ok(audio.to_string_lossy().to_string())
}

pub fn clear_cache(root: &Path) -> Result<()> {
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    Ok(())
}
