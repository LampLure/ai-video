use crate::core::media_probe::probe_video;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMeta {
    pub path: String,
    pub name: String,
    pub hash: String,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub size: u64,
    pub mtime: i64,
}

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "flv", "m4v", "wmv"];

pub fn scan_videos(dir: &str) -> Result<Vec<VideoMeta>> {
    let mut videos = Vec::new();
    for entry in WalkDir::new(dir).follow_links(false).into_iter().filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if !entry.file_type().is_file() || !is_video_path(path) {
            continue;
        }
        videos.push(read_video_meta_fast(path)?);
    }
    videos.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(videos)
}

pub fn read_video_meta(path: &Path) -> Result<VideoMeta> {
    read_video_meta_with_probe(path)
}

pub fn read_video_meta_fast(path: &Path) -> Result<VideoMeta> {
    let meta = fs::metadata(path)?;
    let mtime = meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();
    Ok(VideoMeta {
        path: path.to_string_lossy().to_string(),
        name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
        hash: stable_video_hash(path, meta.len(), mtime),
        duration: 0.0,
        width: 0,
        height: 0,
        fps: 0.0,
        size: meta.len(),
        mtime,
    })
}

pub fn read_video_meta_with_probe(path: &Path) -> Result<VideoMeta> {
    let mut video = read_video_meta_fast(path)?;
    let probe = probe_video(&path.to_string_lossy())?;
    video.duration = probe.duration;
    video.width = probe.width;
    video.height = probe.height;
    video.fps = probe.fps;
    Ok(video)
}

pub fn hydrate_video_meta(video: &VideoMeta) -> Result<VideoMeta> {
    if video.duration > 0.0 && video.width > 0 && video.height > 0 {
        return Ok(video.clone());
    }
    read_video_meta_with_probe(Path::new(&video.path))
}

pub fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| VIDEO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn stable_video_hash(path: &Path, size: u64, mtime: i64) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| PathBuf::from(path));
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    hasher.update(size.to_le_bytes());
    hasher.update(mtime.to_le_bytes());
    hex::encode(&hasher.finalize()[0..16])
}
