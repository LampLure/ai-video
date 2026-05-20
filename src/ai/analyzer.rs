use crate::ai::agent::{handle_agent_request, AgentRequest, AgentResponse, SegmentRequestLimits};
use crate::ai::client::{AiClient, ChatMessage};
use crate::ai::prompts::{read_prompt, RESPONSE_SCHEMA_PROMPT, VIDEO_ANALYSIS_PROMPT, VIDEO_QA_AGENT_PROMPT};
use crate::ai::schema::{clean_model_output, AnalysisResult};
use crate::core::cache_manager::{extract_audio_segment, extract_frames};
use crate::core::settings::AppSettings;
use crate::core::video_manager::VideoMeta;
use crate::db::Database;
use anyhow::{Context, Result};
use std::path::Path;

pub fn analyze_video(video: &VideoMeta, settings: &AppSettings, db_path: &Path) -> Result<AnalysisResult> {
    analyze_video_with_events(video, settings, db_path, |_| {}, |_| {})
}

pub fn analyze_video_with_events<S, D>(video: &VideoMeta, settings: &AppSettings, db_path: &Path, mut on_status: S, mut on_delta: D) -> Result<AnalysisResult>
where
    S: FnMut(&str) + Send,
    D: FnMut(&str) + Send,
{
    on_status("程序计算全局抽帧和音频采样时间点");
    let frame_times = uniform_timestamps(video.duration, settings.max_images);
    let audio_centers = uniform_timestamps(video.duration, settings.max_audio_segments);
    on_status(&format!("debug:视频文件: {}", video.path));
    on_status(&format!("debug:抽帧时间点: {:?}", frame_times));
    on_status(&format!("debug:音频中心时间点: {:?}", audio_centers));

    on_status("程序正在执行视频抽帧");
    let frames = extract_frames(&video.path, &video.hash, &frame_times, settings.image_pixel_limit)?;
    for path in &frames {
        on_status(&format!("debug:\u{1F5BC} {}", path));
    }

    on_status("程序正在执行音频切割");
    let mut audio = Vec::new();
    for center in &audio_centers {
        let path = extract_audio_segment(&video.path, &video.hash, *center, settings.audio_clip_seconds, settings.audio_sample_rate)?;
        on_status(&format!("debug:\u{1F3A7} {}", path));
        audio.push(path);
    }

    on_status("程序正在构造视频简介提示词");
    let mut prompt = String::new();
    prompt.push_str(&read_prompt(VIDEO_ANALYSIS_PROMPT));
    prompt.push_str("\n\n# 当前视频输入\n");
    prompt.push_str(&format!("文件名：{}\n", video.name));
    prompt.push_str(&format!("时长：{:.3}s，分辨率：{}x{}，fps：{:.3}\n", video.duration, video.width, video.height, video.fps));
    prompt.push_str(&format!("抽帧时间点：{:?}\n", frame_times));
    prompt.push_str("\n\n# 输出格式约束\n");
    prompt.push_str(&read_prompt(RESPONSE_SCHEMA_PROMPT));
    on_status(&format!("debug:发送给 AI 的简介提示词字符数: {}", prompt.chars().count()));

    on_status("程序正在发送多模态简介请求");
    let client = AiClient::new(settings.llama_cpp_endpoint.clone(), settings.model_name.clone());
    let raw = client.chat_multimodal_with_callback("", &prompt, &frames, &audio, 0.1, |delta| on_delta(delta))?;
    on_status("程序已收到 AI 简介响应，正在解析 JSON");
    let result = parse_analysis_result(&raw)?;

    on_status("程序正在保存简介到数据库");
    let db = Database::open(db_path)?;
    let video_id = db.upsert_video(video)?;
    db.save_summary(video_id, &result, &settings.model_name)?;
    on_status("程序已保存简介到数据库");
    Ok(result)
}

pub fn ask_video_question(video: &VideoMeta, question: &str, settings: &AppSettings) -> Result<String> {
    ask_video_question_with_events(video, question, settings, |_| {}, |_| {})
}

pub fn ask_video_question_with_events<S, D>(video: &VideoMeta, question: &str, settings: &AppSettings, mut on_status: S, mut on_delta: D) -> Result<String>
where
    S: FnMut(&str) + Send,
    D: FnMut(&str) + Send,
{
    let client = AiClient::new(settings.llama_cpp_endpoint.clone(), settings.model_name.clone());
    on_status("程序正在构造当前视频问答 Agent 提示词");
    let first_prompt = format!(
        "{}\n\n# 当前视频\n视频名：{}\n视频路径：{}\n视频 hash：{}\n视频时长：{:.3}s\n最大图片数：{}\n最大音频段数：{}\n用户问题：{}",
        read_prompt(VIDEO_QA_AGENT_PROMPT), video.name, video.path, video.hash, video.duration, settings.max_images, settings.max_audio_segments, question
    );
    on_status(&format!("debug:视频文件: {}", video.path));
    on_status(&format!("debug:发送给 AI 的首轮 Agent 提示词字符数: {}", first_prompt.chars().count()));
    on_status("程序正在发送首轮 Agent 请求");
    let first = client.chat_streaming_with_callback(vec![ChatMessage { role: "user".to_string(), content: first_prompt.clone() }], 0.2, |_| {})?;
    on_status("程序已收到首轮 Agent 响应，正在判断是否为片段请求 JSON");

    if let Ok(request) = serde_json::from_str::<AgentRequest>(&clean_model_output(&first)) {
        on_status("程序已识别 AI 的片段请求，正在校验请求范围");
        on_status(&format!("debug:AI 片段请求 JSON: {}", clean_model_output(&first)));
        let limits = SegmentRequestLimits::from_settings(video.duration, settings);
        let response = handle_agent_request(request, limits, settings)?;
        match response {
            AgentResponse::Ok { frames, audio } => {
                on_status("程序已完成 AI 请求的视频片段处理");
                for path in &frames {
                    on_status(&format!("debug:\u{1F5BC} {}", path));
                }
                for path in &audio {
                    on_status(&format!("debug:\u{1F3A7} {}", path));
                }
                let evidence_prompt = format!(
                    "用户问题：{}\n\n程序已按你的 JSON 请求准备证据。请只根据这些证据和视频元数据回答。\n视频名：{}\n视频时长：{:.3}s\n请用中文简洁回答。如果证据不足，直接说明不足。",
                    question, video.name, video.duration
                );
                on_status(&format!("debug:发送给 AI 的证据回答提示词字符数: {}", evidence_prompt.chars().count()));
                on_status("程序正在发送片段证据给 AI");
                let answer = client.chat_multimodal_with_callback("", &evidence_prompt, &frames, &audio, 0.2, |delta| on_delta(delta))?;
                on_status("程序已收到 AI 最终回答");
                return Ok(answer);
            }
            AgentResponse::Error { code, message } => {
                on_status("程序拒绝了 AI 的片段请求，正在发送错误信息给 AI");
                on_status(&format!("debug:片段请求错误: {} - {}", code, message));
                let repair_prompt = format!(
                    "你的上一条片段请求被程序拒绝。错误代码：{}。错误信息：{}。请在限制范围内重新请求更少数据，或者直接说明无法回答。用户问题：{}",
                    code, message, question
                );
                let answer = client.chat_streaming_with_callback(
                    vec![
                        ChatMessage { role: "user".to_string(), content: first_prompt },
                        ChatMessage { role: "assistant".to_string(), content: first },
                        ChatMessage { role: "user".to_string(), content: repair_prompt },
                    ],
                    0.2,
                    |delta| on_delta(delta),
                )?;
                on_status("程序已收到 AI 最终回答");
                return Ok(answer);
            }
        }
    }

    on_status("首轮 Agent 响应不是片段请求，程序直接展示该回答");
    Ok(first)
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
        for scene in &result.scenes { out.push_str(&format!("  {:.1}s-{:.1}s：{}\n", scene.start, scene.end, scene.description)); }
    }
    out
}

pub fn uniform_timestamps(duration: f64, count: usize) -> Vec<f64> {
    if count == 0 || duration <= 0.0 { return Vec::new(); }
    if count == 1 { return vec![(duration * 0.5).max(0.0)]; }
    let end = duration.max(0.1);
    (0..count).map(|idx| (idx as f64 / (count - 1) as f64) * end).collect()
}
