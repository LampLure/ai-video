use crate::ai::schema::AnalysisResult;
use crate::core::settings::AppSettings;
use crate::core::video_manager::VideoMeta;
use crate::db::Database;
use anyhow::Result;
use std::path::Path;

pub fn analyze_video(video: &VideoMeta, settings: &AppSettings, db_path: &Path) -> Result<AnalysisResult> {
    let result = AnalysisResult { title: video.name.clone(), summary: "AI analysis placeholder".to_string(), tags: vec!["待分析".to_string()], ..Default::default() };
    let db = Database::open(db_path)?;
    let video_id = db.upsert_video(video)?;
    db.save_summary(video_id, &result, &settings.model_name)?;
    Ok(result)
}

pub fn ask_video_question(_video: &VideoMeta, question: &str, _settings: &AppSettings) -> Result<String> {
    Ok(format!("已接收问题：{}", question))
}

pub fn render_analysis_text(result: &AnalysisResult) -> String {
    format!("标题：{}\n简介：{}\n标签：{}", result.title, result.summary, result.tags.join(" / "))
}
