use crate::ai::agent::{handle_agent_request, AgentRequest, SegmentRequestLimits};
use crate::ai::queue::{AnalysisQueue, QueueState};
use crate::ai::{analyze_video, ask_video_question, AnalysisResult};
use crate::core::cache_manager::{extract_audio_segment, extract_frames};
use crate::core::settings::AppSettings;
use crate::core::video_manager::{scan_videos as scan_videos_inner, VideoMeta};
use crate::db::{Database, SearchResult};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static SETTINGS: OnceLock<Mutex<AppSettings>> = OnceLock::new();
static QUEUE: OnceLock<Mutex<AnalysisQueue>> = OnceLock::new();

fn settings_cell() -> &'static Mutex<AppSettings> {
    SETTINGS.get_or_init(|| Mutex::new(AppSettings::default()))
}

fn queue_cell() -> &'static Mutex<AnalysisQueue> {
    QUEUE.get_or_init(|| Mutex::new(AnalysisQueue::new()))
}

fn db_path() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("ai-video").join("ai-video.sqlite3")
}

fn to_app_error(err: anyhow::Error) -> String { err.to_string() }

pub fn get_default_settings() -> AppSettings {
    settings_cell().lock().map(|v| v.clone()).unwrap_or_default()
}

pub fn save_settings(settings: AppSettings) -> Result<AppSettings, String> {
    *settings_cell().lock().map_err(|e| e.to_string())? = settings.clone();
    Ok(settings)
}

pub fn scan_videos(dir: String) -> Result<Vec<VideoMeta>, String> {
    let videos = scan_videos_inner(&dir).map_err(to_app_error)?;
    let db = Database::open(db_path()).map_err(to_app_error)?;
    for video in &videos {
        let _ = db.upsert_video(video);
    }
    Ok(videos)
}

pub fn init_database() -> Result<String, String> {
    let path = db_path();
    Database::open(&path).map_err(to_app_error)?;
    Ok(path.to_string_lossy().to_string())
}

pub fn search_videos(query: String, limit: Option<usize>) -> Result<Vec<SearchResult>, String> {
    let db = Database::open(db_path()).map_err(to_app_error)?;
    db.search(&query, limit.unwrap_or(50)).map_err(to_app_error)
}

pub fn analyze_current_video(video: VideoMeta) -> Result<AnalysisResult, String> {
    let settings = settings_cell().lock().map_err(|e| e.to_string())?.clone();
    analyze_video(&video, &settings, &db_path()).map_err(to_app_error)
}

pub fn ask_video(video: VideoMeta, question: String) -> Result<String, String> {
    let settings = settings_cell().lock().map_err(|e| e.to_string())?.clone();
    ask_video_question(&video, &question, &settings).map_err(to_app_error)
}

pub fn start_analysis_queue(videos: Vec<VideoMeta>, start_index: usize) -> Result<QueueState, String> {
    let mut queue = AnalysisQueue::load_from(videos, start_index);
    queue.start();
    let state = queue.state();
    *queue_cell().lock().map_err(|e| e.to_string())? = queue;
    Ok(state)
}

pub fn pause_analysis_queue() -> Result<QueueState, String> {
    let mut queue = queue_cell().lock().map_err(|e| e.to_string())?;
    queue.pause();
    Ok(queue.state())
}

pub fn ask_current_video(question: String) -> Result<String, String> {
    let queue = queue_cell().lock().map_err(|e| e.to_string())?;
    if !queue.can_user_ask() {
        return Err("后台分析未暂停，当前禁止用户提问".to_string());
    }
    let current = queue.current().map(|job| job.video.name.clone()).unwrap_or_else(|| "当前视频".to_string());
    Ok(format!("已接收针对 {current} 的问题：{question}"))
}

pub fn prepare_video_segment(request: AgentRequest, duration: f64) -> Result<serde_json::Value, String> {
    let settings = settings_cell().lock().map_err(|e| e.to_string())?.clone();
    let limits = SegmentRequestLimits::from_settings(duration, &settings);
    let response = handle_agent_request(request, limits, &settings).map_err(to_app_error)?;
    serde_json::to_value(response).map_err(|e| e.to_string())
}

pub fn prepare_frames_for_times(video_path: &str, video_hash: &str, times: &[f64]) -> Result<Vec<String>> {
    let settings = settings_cell().lock().unwrap().clone();
    extract_frames(video_path, video_hash, times, settings.image_pixel_limit)
}

pub fn prepare_audio_for_center(video_path: &str, video_hash: &str, center: f64) -> Result<String> {
    let settings = settings_cell().lock().unwrap().clone();
    extract_audio_segment(video_path, video_hash, center, settings.audio_clip_seconds, settings.audio_sample_rate)
}
