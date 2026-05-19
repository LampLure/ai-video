use crate::ai::client::{AiClient, ChatMessage};
use crate::ai::schema::{clean_model_output, response_schema_prompt, AnalysisResult};
use crate::core::cache_manager::{extract_audio_segment, extract_frames};
use crate::core::settings::AppSettings;
use crate::core::video_manager::VideoMeta;
use crate::db::Database;
use anyhow::{Context, Result};
use std::path::Path;

pub fn analyze_video(video: &VideoMeta, settings: &AppSettings, db_path: &Path) -> Result<AnalysisResult> {
    let frame_times = uniform_timestamps(video.duration, settings.max_images);
    let audio_centers = uniform_timestamps(video.duration, settings.max_audio_segments);
    let frames = extract_frames(&video.path, &video.hash, &frame_times, settings.image_pixel_limit)?;
    let mut audio = Vec::new();
    for center in &audio_centers {
        audio.push(extract_audio_segment(&video.path, &video.hash, *center, settings.audio_clip_seconds, settings.audio_sample_rate)?);
    }

    let mut prompt = String::new();
    prompt.push_str("请根据给定的视频采样文件生成中文视频简介。\n");
    prompt.push_str(&format!("文件名：{}\n", video.name));
    prompt.push_str(&format!("时长：{:.3}s，分辨率：{}x{}，fps：{:.3}\n", video.duration, video.width, video.height, video.fps));
    prompt.push_str(&format!("抽帧时间点：{:?}\n", frame_times));
    prompt.push_str(&format!("图片文件：{}\n", frames.join(", ")));
    prompt.push_str(&format!("音频文件：{}\n", audio.join(", ")));
    prompt.push_str(response_schema_prompt());

    let client = AiClient::new(settings.llama_cpp_endpoint.clone(), settings.model_name.clone());
    let raw = client.chat(vec![ChatMessage { role: "user".to_string(), content: prompt }], 0.1)?;
    let result = parse_analysis_result(&raw)?;

    let db = Database::open(db_path)?;
    let video_id = db.upsert_video(video)?;
    db.save_summary(video_id, &result, &settings.model_name)?;
    Ok(result)
}

pub fn ask_video_question(video: &VideoMeta, question: &str, settings: &AppSettings) -> Result<String> {
    let client = AiClient::new(settings.llama_cpp_endpoint.clone(), settings.model_name.clone());
    let prompt = format!("你正在回答当前视频的问题。视频名：{}。视频时长：{:.3}s。最多可请求图片 {} 张，音频 {} 段。用户问题：{}。请用中文简洁回答；如果需要更多证据，请说明需要查看的时间段。", video.name, video.duration, settings.max_images, settings.max_audio_segments, question);
    client.chat(vec![ChatMessage { role: "user".to_string(), content: prompt }], 0.2)
}

pub fn parse_analysis_result(raw: &str) -> Result<AnalysisResult> {
    let cleaned = clean_model_output(raw);
    serde_json::from_str(&cleaned).with_context(|| format!("AI 返回了无效的简介 JSON：{cleaned}"))
}

pub fn render_analysis_text(result: &AnalysisResult) -> String {
    let mut out = String::new();
    out.push_str(&format!("标题：{}\n", result.title));
    out.push_str(&format!("简介：{}\n", result.summary));
    if !result.tags.is_empty() { out.push_str(&format!("标签：{}\n", result.tags.join(" / "))); }
    if !result.scenes.is_empty() {
        out.push_str("场景：\n");
        for scene in &result.scenes {
            out.push_str(&format!("  {:.1}s-{:.1}s：{}\n", scene.start, scene.end, scene.description));
        }
    }
    out
}

pub fn uniform_timestamps(duration: f64, count: usize) -> Vec<f64> {
    if count == 0 || duration <= 0.0 { return Vec::new(); }
    if count == 1 { return vec![(duration * 0.5).max(0.0)]; }
    let end = duration.max(0.1);
    (0..count).map(|idx| (idx as f64 / (count - 1) as f64) * end).collect()
}
