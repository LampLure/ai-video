use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
    run_ffmpeg(&[
        "-y", "-hide_banner", "-loglevel", "error", "-ss", &timestamp,
        "-i", video_path, "-frames:v", "1", "-q:v", "4", &paths.thumb,
    ], "thumbnail extraction")?;
    Ok(paths.thumb)
}

pub fn extract_frames(video_path: &str, video_hash: &str, timestamps: &[f64], pixel_limit: u32) -> Result<Vec<String>> {
    let root = default_cache_root();
    let paths = ensure_cache_layout(&root, video_hash)?;
    let mut out = Vec::new();
    let pixel_limit = pixel_limit.max(1);
    let scale = format!("scale='if(gt(iw*ih,{0}),trunc(iw*sqrt({0}/(iw*ih))/2)*2,iw)':'if(gt(iw*ih,{0}),trunc(ih*sqrt({0}/(iw*ih))/2)*2,ih)'", pixel_limit);
    for (idx, ts) in timestamps.iter().enumerate() {
        let millis = (ts.max(0.0) * 1000.0).round() as u64;
        let frame = Path::new(&paths.frames_dir).join(format!("frame_{idx:03}_{millis}ms.jpg"));
        if frame.exists() {
            out.push(frame.to_string_lossy().to_string());
            continue;
        }
        let frame_path = frame.to_string_lossy().to_string();
        let timestamp = format!("{:.3}", ts.max(0.0));
        run_ffmpeg(&[
            "-y", "-hide_banner", "-loglevel", "error", "-ss", &timestamp,
            "-i", video_path, "-frames:v", "1", "-vf", &scale, "-q:v", "5", &frame_path,
        ], "frame extraction")?;
        out.push(frame_path);
    }
    Ok(out)
}

pub fn extract_audio_segment(video_path: &str, video_hash: &str, center_seconds: f64, clip_seconds: f32, sample_rate: u32) -> Result<String> {
    let root = default_cache_root();
    let paths = ensure_cache_layout(&root, video_hash)?;
    let safe_center = center_seconds.max(0.0);
    let clip_seconds = clip_seconds.max(0.1);
    let sample_rate = sample_rate.max(8000);
    let start = (safe_center - (clip_seconds as f64 / 2.0)).max(0.0);
    let audio = Path::new(&paths.audio).with_file_name(format!("{}_{}ms_{}hz.wav", video_hash, (safe_center * 1000.0).round() as u64, sample_rate));
    if audio.exists() {
        return Ok(audio.to_string_lossy().to_string());
    }
    let audio_path = audio.to_string_lossy().to_string();
    run_ffmpeg(&[
        "-y", "-hide_banner", "-loglevel", "error", "-ss", &format!("{start:.3}"),
        "-i", video_path, "-t", &format!("{clip_seconds:.3}"), "-ac", "1", "-ar", &sample_rate.to_string(),
        "-vn", &audio_path,
    ], "audio extraction")?;
    Ok(audio_path)
}

pub fn clear_cache(root: &Path) -> Result<()> {
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    Ok(())
}

fn run_ffmpeg(args: &[&str], operation: &str) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to launch ffmpeg for {operation}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg {operation} failed: {}", stderr.trim());
    }
    Ok(())
}
