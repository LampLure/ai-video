use crate::ai::analyzer::{analyze_video_with_events, ask_video_question_with_events};
use crate::ai::prompts;
use crate::ai::{render_analysis_text, AnalysisResult};
use crate::core::cache_manager::{default_cache_root, enforce_cache_size_limit};
use crate::core::config;
use crate::core::settings::{AppSettings, CacheSwitchPolicy};
use crate::core::video_manager::{hydrate_video_meta, scan_videos, VideoMeta};
use crate::db::{Database, SearchResult};
use crate::models;
use crate::ui::status::{AiStatusState, QaEvent, STATUS_AI_ANALYZING, STATUS_AI_ANSWER_COMPLETE, STATUS_AI_ANSWERING, STATUS_AI_RECEIVED_USER_MESSAGE, STATUS_PROGRAM_REQUEST_OK};
use crate::ui::status_panel::show_ai_status_panel;
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScreenMode { Overview, Playback, AiAnalysis, Settings }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsSection { Model, Prompts, AiLimits, Media, Cache, Debug }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelCheckKind { Start, Stop }

#[derive(Debug, Clone)]
struct PendingModelCheck { due: Instant, kind: ModelCheckKind }

#[derive(Debug)]
enum AiEvent {
    Status(String),
    Debug(String),
    Message(String),
    Select(usize),
    Completed { index: usize, name: String, result: AnalysisResult },
    Error { index: usize, name: String, error: String },
    Finished,
}

pub struct AiVideoApp {
    mode: ScreenMode,
    settings_section: SettingsSection,
    settings: AppSettings,
    folder: Option<PathBuf>,
    videos: Vec<VideoMeta>,
    selected_index: Option<usize>,
    scan_status: String,
    search_query: String,
    search_results: Vec<SearchResult>,
    chat_log: Vec<String>,
    user_question: String,
    db_path: PathBuf,
    ai_running: bool,
    ai_paused: bool,
    ai_rx: Option<mpsc::Receiver<AiEvent>>,
    ai_cancel: Option<Arc<AtomicBool>>,
    qa_rx: Option<mpsc::Receiver<QaEvent>>,
    qa_running: bool,
    ai_status: AiStatusState,
    qa_stream_index: Option<usize>,
    thumbnails: HashMap<String, egui::TextureHandle>,
    thumbnail_errors: HashMap<String, String>,
    playback_position: f64,
    playback_playing: bool,
    playback_last_tick: Option<Instant>,
    playback_frames: HashMap<String, egui::TextureHandle>,
    playback_frame_errors: HashMap<String, String>,
    model_scripts: Vec<PathBuf>,
    selected_model_script: Option<PathBuf>,
    model_child: Option<Child>,
    model_status: String,
    pending_model_check: Option<PendingModelCheck>,
    model_notice_open: bool,
    model_notice_title: String,
    model_notice_body: String,
    model_notice_dismiss_at: Option<Instant>,
    bottom_panel_height: f32,
    debug_log: Vec<String>,
    debug_follow: bool,
    debug_follow_blocked_until: Option<Instant>,
    pending_folder: Arc<Mutex<Option<PathBuf>>>,
    initialized: bool,
}

impl Default for AiVideoApp {
    fn default() -> Self {
        let scripts = models::list_model_scripts();
        let selected_script = scripts.first().cloned();
        let _ = prompts::ensure_prompt_files();
        Self {
            mode: ScreenMode::Overview,
            settings_section: SettingsSection::Model,
            settings: config::load_settings(),
            folder: None,
            videos: Vec::new(),
            selected_index: None,
            scan_status: "请选择一个视频文件夹".to_string(),
            search_query: String::new(),
            search_results: Vec::new(),
            chat_log: vec!["系统：AI Video 已启动。".to_string()],
            user_question: String::new(),
            db_path: default_db_path(),
            ai_running: false,
            ai_paused: false,
            ai_rx: None,
            ai_cancel: None,
            qa_rx: None,
            qa_running: false,
            ai_status: AiStatusState::default(),
            qa_stream_index: None,
            thumbnails: HashMap::new(),
            thumbnail_errors: HashMap::new(),
            playback_position: 0.0,
            playback_playing: false,
            playback_last_tick: None,
            playback_frames: HashMap::new(),
            playback_frame_errors: HashMap::new(),
            model_scripts: scripts,
            selected_model_script: selected_script,
            model_child: None,
            model_status: format!("模型目录：{}", models::ensure_models_dir().display()),
            pending_model_check: None,
            model_notice_open: false,
            model_notice_title: String::new(),
            model_notice_body: String::new(),
            model_notice_dismiss_at: None,
            bottom_panel_height: 86.0,
            debug_log: Vec::new(),
            debug_follow: true,
            debug_follow_blocked_until: None,
            pending_folder: Arc::new(Mutex::new(None)),
            initialized: false,
        }
    }
}

impl eframe::App for AiVideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.initialized {
            self.initialized = true;
            if let Some(ref last) = self.settings.last_folder {
                let p = PathBuf::from(last);
                if p.exists() { let _ = self.pending_folder.lock().map(|mut pf| *pf = Some(p)); }
            }
        }
        let folder_to_open = self.pending_folder.lock().ok().and_then(|mut p| p.take());
        if let Some(folder) = folder_to_open {
            self.open_folder(folder);
        }
        apply_gray_theme(ctx);
        self.tick_playback(ctx);
        self.poll_model_process();
        self.poll_model_health();
        self.poll_ai_events(ctx);
        self.poll_qa_events(ctx);
        self.show_model_notice(ctx);
        self.show_debug_window(ctx);

        egui::TopBottomPanel::top("top_bar").frame(panel_frame()).show(ctx, |ui| self.top_bar(ui));
        egui::SidePanel::left("control_panel")
            .resizable(true)
            .default_width(280.0)
            .width_range(96.0..=650.0)
            .frame(panel_frame())
            .show(ctx, |ui| self.left_control_panel(ui));

        if self.mode == ScreenMode::AiAnalysis {
            egui::SidePanel::right("ai_video_strip")
                .resizable(true)
                .default_width(230.0)
                .width_range(90.0..=560.0)
                .frame(panel_frame())
                .show(ctx, |ui| self.right_ai_video_list(ui, ctx));
        }

        if matches!(self.mode, ScreenMode::Overview | ScreenMode::Playback | ScreenMode::AiAnalysis) {
            egui::TopBottomPanel::bottom("bottom_input")
                .exact_height(self.bottom_panel_height.clamp(36.0, 720.0))
                .frame(panel_frame())
                .show(ctx, |ui| self.bottom_input_area(ui));
        }

        egui::CentralPanel::default().frame(content_frame()).show(ctx, |ui| match self.mode {
            ScreenMode::Overview => self.overview_grid(ui, ctx),
            ScreenMode::Playback => self.playback_view(ui, ctx),
            ScreenMode::AiAnalysis => self.ai_analysis_view(ui, ctx),
            ScreenMode::Settings => self.settings_view(ui),
        });
    }
}

impl AiVideoApp {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("AI Video");
            ui.separator();
            ui.label(match self.mode { ScreenMode::Overview => "总览", ScreenMode::Playback => "播放", ScreenMode::AiAnalysis => "AI 分析", ScreenMode::Settings => "设置" });
            ui.separator();
            ui.label(&self.scan_status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let state = if self.ai_running { "AI 队列运行中" } else if self.qa_running { "AI 问答中" } else { "就绪" };
                ui.label(state);
            });
        });
    }

    fn left_control_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("控制面板");
        ui.separator();
        if nav_button(ui, "选择文件夹").clicked() {
            let pending = self.pending_folder.clone();
            std::thread::spawn(move || {
                let folder = rfd::FileDialog::new().pick_folder();
                if let Ok(mut p) = pending.lock() { *p = folder; }
            });
        }
        if nav_button(ui, "分析文件/生成缩略图").clicked() { self.generate_visible_thumbnails(); }
        egui::Frame::group(ui.style()).show(ui, |ui| self.model_panel(ui));
        ui.add_space(8.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new("当前目录").strong());
            ui.small(elide_middle(&self.folder.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "未选择".to_string()), 32));
        });
        ui.add_space(8.0);
        if nav_button(ui, "媒体总览").clicked() { self.switch_mode(ScreenMode::Overview); }
        if nav_button(ui, "进入播放").clicked() { if self.selected_index.is_none() && !self.videos.is_empty() { self.select_video(0); } self.switch_mode(ScreenMode::Playback); }
        if nav_button(ui, if self.mode == ScreenMode::AiAnalysis { "退出 AI 分析" } else { "AI 分析" }).clicked() {
            if self.mode == ScreenMode::AiAnalysis { self.switch_mode(ScreenMode::Overview); } else { if self.selected_index.is_none() && !self.videos.is_empty() { self.select_video(0); } self.switch_mode(ScreenMode::AiAnalysis); }
        }
        if nav_button(ui, "设置").clicked() { self.mode = ScreenMode::Settings; }
        ui.add_space(10.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new("目录信息").strong());
            ui.label(format!("视频数：{}", self.videos.len()));
            if let Some(idx) = self.selected_index.and_then(|i| self.videos.get(i).map(|_| i)) { ui.small(elide_middle(&format!("当前：{}", self.videos[idx].name), 36)); }
        });
    }

    fn model_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("模型启动").strong());
        ui.small(elide_middle(&format!("目录：{}", models::ensure_models_dir().display()), 34));
        ui.horizontal(|ui| {
            if ui.button("刷新").clicked() { self.refresh_model_scripts(); }
            let can_start = self.model_child.is_none() && !self.has_pending_model_check(ModelCheckKind::Start);
            if ui.add_enabled(can_start, egui::Button::new("启动")).clicked() { self.start_selected_model(); }
            if ui.button("终止").clicked() { self.stop_model(); }
        });
        let selected_text = self.selected_model_script.as_ref().and_then(|p| p.file_name()).and_then(|s| s.to_str()).map(|s| elide_middle(s, 22)).unwrap_or_else(|| "未选择启动文件".to_string());
        egui::ComboBox::from_id_salt("model_script_combo").selected_text(selected_text).width((ui.available_width() - 4.0).max(60.0)).show_ui(ui, |ui| {
            for path in self.model_scripts.clone() {
                let full = path.display().to_string();
                let label = path.file_name().and_then(|s| s.to_str()).map(|s| elide_middle(s, 40)).unwrap_or_else(|| "unknown".to_string());
                ui.selectable_value(&mut self.selected_model_script, Some(path), label).on_hover_text(full);
            }
        });
        ui.small(elide_middle(&self.model_status, 42));
    }

    fn overview_grid(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| { ui.heading("媒体总览"); ui.separator(); ui.label(format!("{} 个视频", self.filtered_video_indices().len())); });
        if !self.search_results.is_empty() { self.search_results_panel(ui); }
        ui.add_space(12.0);
        if self.videos.is_empty() { ui.centered_and_justified(|ui| ui.label("点击左侧“选择文件夹”后，视频会以卡片网格排列在这里。")); return; }
        let card_width = 260.0;
        let card_height = 245.0;
        let spacing = 16.0;
        let cols = ((ui.available_width() + spacing) / (card_width + spacing)).floor().max(1.0) as usize;
        let indices = self.filtered_video_indices();
        let row_height = card_height + 14.0;
        let total_rows = (indices.len() + cols - 1) / cols;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_height, total_rows, |ui, row_range| {
            for row_idx in row_range {
                ui.horizontal_top(|ui| {
                    let start = row_idx * cols;
                    let end = ((row_idx + 1) * cols).min(indices.len());
                    for &idx in &indices[start..end] {
                        self.overview_video_card(ui, ctx, idx, egui::vec2(card_width, card_height));
                        ui.add_space(spacing);
                    }
                });
            }
        });
    }

    fn search_results_panel(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label(egui::RichText::new("简介搜索结果").strong());
        let results = self.search_results.clone();
        for result in results.iter().take(8) {
            if ui.button(format!("{} - {}", result.name, result.title.clone().unwrap_or_default())).clicked() {
                if let Some(idx) = self.videos.iter().position(|v| v.path == result.path) { self.select_video(idx); self.switch_mode(ScreenMode::Playback); }
            }
            if let Some(summary) = &result.summary { ui.small(summary); }
        }
        ui.separator();
    }

    fn overview_video_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, idx: usize, size: egui::Vec2) {
        self.hydrate_video_maybe(idx);
        let selected = self.selected_index == Some(idx);
        let video = self.videos[idx].clone();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        ui.painter().rect_filled(rect, 6.0, if selected { egui::Color32::from_gray(44) } else { egui::Color32::from_gray(26) });
        ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.0, egui::Color32::from_gray(if selected { 95 } else { 42 })), egui::StrokeKind::Outside);
        let thumb_rect = egui::Rect::from_min_size(rect.min + egui::vec2(10.0, 10.0), egui::vec2(size.x - 20.0, 170.0));
        self.paint_thumbnail(ui, ctx, &video, thumb_rect);
        ui.painter().text(egui::pos2(rect.center().x, thumb_rect.max.y + 20.0), egui::Align2::CENTER_CENTER, &video.name, egui::TextStyle::Button.resolve(ui.style()), egui::Color32::from_gray(235));
        ui.painter().text(egui::pos2(rect.center().x, thumb_rect.max.y + 42.0), egui::Align2::CENTER_CENTER, format_duration(video.duration), egui::TextStyle::Small.resolve(ui.style()), egui::Color32::from_gray(165));
        if response.clicked() { self.select_video(idx); self.switch_mode(ScreenMode::Playback); }
    }

    fn playback_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| { ui.heading("视频播放界面"); ui.separator(); if ui.button("返回媒体总览").clicked() { self.switch_mode(ScreenMode::Overview); } if ui.button("切到 AI 分析").clicked() { self.switch_mode(ScreenMode::AiAnalysis); } });
        ui.separator();
        self.preview_panel(ui, ctx, "请先在媒体总览中选择一个视频。");
        self.current_summary_panel(ui);
    }

    fn ai_analysis_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("AI 分析模式");
        ui.separator();
        self.preview_panel(ui, ctx, "AI 分析模式：点击右侧缩略图切换预览区");
        self.current_summary_panel(ui);
        ui.add_space(10.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new("队列与控制").strong());
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.ai_running, egui::Button::new("开始 AI 分析队列")).clicked() { self.start_analysis_queue(); }
                if ui.add_enabled(self.ai_running, egui::Button::new("暂停队列，允许提问")).clicked() { self.pause_analysis_queue(); }
            });
            let state = if self.ai_running { "后台分析中，禁止提问" } else if self.ai_paused { "已暂停，可提问" } else { "未开始" };
            ui.label(format!("当前状态：{state}"));
        });
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.separator();
        ui.horizontal_top(|ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(176.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("设置导航").strong());
                    ui.add_space(6.0);
                    self.settings_nav_button(ui, SettingsSection::Model, "模型启动");
                    self.settings_nav_button(ui, SettingsSection::Prompts, "提示词/Agent");
                    self.settings_nav_button(ui, SettingsSection::AiLimits, "AI 限制");
                    self.settings_nav_button(ui, SettingsSection::Media, "媒体处理");
                    self.settings_nav_button(ui, SettingsSection::Cache, "缓存策略");
                    self.settings_nav_button(ui, SettingsSection::Debug, "调试模式");
                    ui.add_space(16.0);
                    if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("保存设置")).clicked() { self.save_settings_with_notice(); }
                });
            });
            ui.add_space(12.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 8.0).max(360.0));
                settings_card(ui, |ui| match self.settings_section {
                    SettingsSection::Model => self.settings_model(ui),
                    SettingsSection::Prompts => self.settings_prompts(ui),
                    SettingsSection::AiLimits => self.settings_ai_limits(ui),
                    SettingsSection::Media => self.settings_media(ui),
                    SettingsSection::Cache => self.settings_cache(ui),
                    SettingsSection::Debug => self.settings_debug(ui),
                });
            });
        });
    }

    fn settings_model(&mut self, ui: &mut egui::Ui) {
        ui.heading("模型启动");
        ui.add_space(8.0);
        ui.label(format!("将 llama.cpp 启动脚本放到：{}", models::ensure_models_dir().display()));
        ui.add_space(10.0);
        setting_text(ui, "接口", &mut self.settings.llama_cpp_endpoint);
        setting_text(ui, "模型名", &mut self.settings.model_name);
        ui.add_space(12.0);
        self.model_panel(ui);
    }

    fn settings_prompts(&mut self, ui: &mut egui::Ui) {
        ui.heading("提示词 / Skill / Agent");
        ui.add_space(8.0);
        ui.label(format!("提示词目录：{}", prompts::ensure_prompt_files().display()));
        ui.add_space(10.0);
        prompt_button(ui, "修改视频简介生成提示词", prompts::VIDEO_ANALYSIS_PROMPT, self);
        prompt_button(ui, "修改当前视频问答 Agent 限制文本", prompts::VIDEO_QA_AGENT_PROMPT, self);
        prompt_button(ui, "修改 AI 简介 JSON 输出格式", prompts::RESPONSE_SCHEMA_PROMPT, self);
        ui.add_space(10.0);
        ui.label("修改并保存文档后，后续 AI 分析会读取新的提示词。已在运行中的单次请求不会被中途替换。 ");
    }

    fn settings_ai_limits(&mut self, ui: &mut egui::Ui) {
        ui.heading("AI 限制");
        ui.add_space(8.0);
        setting_slider_usize(ui, "第一次分析发送图片数 / 请求图片上限", &mut self.settings.max_images, 1..=64);
        setting_slider_usize(ui, "音频段数上限", &mut self.settings.max_audio_segments, 0..=32);
        setting_slider_usize(ui, "最大上下文 token 长度", &mut self.settings.max_context_tokens, 1024..=65536);
    }

    fn settings_media(&mut self, ui: &mut egui::Ui) {
        ui.heading("媒体处理");
        ui.add_space(8.0);
        setting_slider_f32(ui, "音频截取长度/s", &mut self.settings.audio_clip_seconds, 1.0..=30.0);
        setting_slider_u32(ui, "图片压缩总像素上限", &mut self.settings.image_pixel_limit, 1000..=100000);
        setting_slider_u32(ui, "音频采样率", &mut self.settings.audio_sample_rate, 8000..=48000);
    }

    fn settings_cache(&mut self, ui: &mut egui::Ui) {
        ui.heading("缓存策略");
        ui.add_space(8.0);
        setting_slider_u64(ui, "缓存大小上限 / MB", &mut self.settings.cache_size_limit_mb, 128..=65536);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("清理到大小上限").clicked() { self.clean_cache_to_limit(); }
            if ui.button("保存设置").clicked() { self.save_settings_with_notice(); }
        });
        ui.add_space(8.0);
        ui.label(format!("缓存目录：{}", default_cache_root().display()));
        ui.label("程序会删除最旧缓存文件，直到总大小低于上限。 ");
    }

    fn settings_debug(&mut self, ui: &mut egui::Ui) {
        ui.heading("调试模式");
        ui.add_space(8.0);
        ui.checkbox(&mut self.settings.debug_mode, "显示原始 JSON");
        ui.label("非调试模式下，聊天栏仅显示清洗后的文本。");
        ui.separator();
        ui.label("切换文件夹时缩略图缓存策略");
        let mut clear_on_switch = matches!(self.settings.cache_switch_policy, CacheSwitchPolicy::ClearOnFolderChange);
        if ui.checkbox(&mut clear_on_switch, "切换文件夹时清空旧缓存").changed() {
            self.settings.cache_switch_policy = if clear_on_switch { CacheSwitchPolicy::ClearOnFolderChange } else { CacheSwitchPolicy::Keep };
        }
    }

    fn right_ai_video_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("AI 队列");
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 136.0, self.videos.len(), |ui, range| {
            for idx in range { self.side_video_card(ui, ctx, idx); }
        });
    }

    fn bottom_input_area(&mut self, ui: &mut egui::Ui) {
        self.bottom_resize_handle(ui);
        ui.separator();
        if self.mode == ScreenMode::Overview {
            ui.horizontal(|ui| { ui.label("搜索"); let input_width = (ui.available_width() - 90.0).max(120.0); ui.add_sized([input_width, 26.0], egui::TextEdit::singleline(&mut self.search_query).hint_text("搜索文件名或已生成的 AI 简介")); if ui.button("搜索").clicked() { self.search_current_database(); } });
            return;
        }
        self.bottom_chat(ui);
    }

    fn bottom_resize_handle(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), egui::Sense::click_and_drag());
        ui.painter().rect_filled(rect.shrink2(egui::vec2(0.0, 2.5)), 2.0, egui::Color32::from_gray(58));
        if response.dragged() { self.bottom_panel_height = (self.bottom_panel_height - response.drag_delta().y).clamp(36.0, 720.0); ui.ctx().request_repaint(); }
        response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
    }

    fn bottom_chat(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("AI 对话");
            ui.separator();
            let prompt_state = if self.ai_running { "后台分析中：输入禁用" } else if self.qa_running { "问答请求中" } else { "可提问当前视频" };
            ui.label(prompt_state);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_refresh = !self.ai_running && !self.qa_running;
                if ui.add_enabled(can_refresh, egui::Button::new("刷新对话")).clicked() { self.reset_chat_context(); }
            });
        });
        show_ai_status_panel(ui, &self.ai_status);
        let log_height = (ui.available_height() - 78.0).max(50.0);
        egui::ScrollArea::vertical().max_height(log_height).auto_shrink([false, false]).show(ui, |ui| {
            let keep_from = self.chat_log.len().saturating_sub(80);
            for line in &self.chat_log[keep_from..] { egui::Frame::group(ui.style()).show(ui, |ui| { ui.label(line); }); ui.add_space(4.0); }
        });
        ui.horizontal(|ui| {
            let disabled = self.ai_running || self.qa_running;
            ui.add_enabled_ui(!disabled, |ui| {
                ui.text_edit_singleline(&mut self.user_question);
                if ui.button("发送").clicked() && !self.user_question.trim().is_empty() { self.send_current_question(); }
            });
        });
    }

    fn preview_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, empty_hint: &str) {
        if let Some(video) = self.current_video().cloned() {
            let available_width = ui.available_width().max(320.0);
            let preview_height = (available_width * 9.0 / 16.0).clamp(220.0, 620.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(available_width, preview_height), egui::Sense::hover());
            ui.painter().rect_filled(rect, 8.0, egui::Color32::BLACK);
            ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, egui::Color32::from_gray(70)), egui::StrokeKind::Outside);
            self.paint_playback_frame(ui, ctx, &video, rect.shrink(8.0));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let button_text = if self.playback_playing { "暂停" } else { "播放预览" };
                if ui.button(button_text).clicked() { self.playback_playing = !self.playback_playing; self.playback_last_tick = Some(Instant::now()); }
                if ui.button("停止").clicked() { self.playback_playing = false; self.playback_position = 0.0; self.playback_last_tick = None; }
                if ui.button("用 mpv 播放").clicked() { match crate::core::player::open_with_mpv(&video.path, self.playback_position) { Ok(()) => self.chat_log.push(format!("系统：已调用 mpv 播放 {}", video.name)), Err(err) => self.chat_log.push(format!("系统：mpv 启动失败：{err}")), } }
                let duration = video.duration.max(0.1);
                let pos_label = format!("{} / {}", format_duration(self.playback_position), format_duration(video.duration));
                let slider = egui::Slider::new(&mut self.playback_position, 0.0..=duration).show_value(false).text(pos_label);
                if ui.add(slider).changed() { self.playback_last_tick = Some(Instant::now()); }
            });
            ui.heading(&video.name);
            ui.label(format_duration(video.duration));
            ui.small("说明：当前内置区域仍是抽帧预览；完整播放请用 mpv 外部窗口。后续再接 libmpv 内嵌。 ");
        } else { ui.centered_and_justified(|ui| ui.label(empty_hint)); }
    }

    fn current_summary_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(video) = self.current_video() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new("AI 简介").strong());
                match Database::open(&self.db_path).and_then(|db| db.get_summary_by_hash(&video.hash)) {
                    Ok(Some(summary)) => { ui.label(render_analysis_text(&summary)); }
                    Ok(None) => { ui.label("当前视频尚未生成简介。"); }
                    Err(err) => { ui.label(format!("读取简介失败：{err}")); }
                }
            });
        }
    }

    fn side_video_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, idx: usize) {
        self.hydrate_video_maybe(idx);
        let selected = self.selected_index == Some(idx);
        let video = self.videos[idx].clone();
        let size = egui::vec2(ui.available_width().max(90.0), 130.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        ui.painter().rect_filled(rect, 5.0, if selected { egui::Color32::from_gray(58) } else { egui::Color32::from_gray(32) });
        ui.painter().rect_stroke(rect, 5.0, egui::Stroke::new(1.0, egui::Color32::from_gray(if selected { 100 } else { 45 })), egui::StrokeKind::Outside);
        let thumb_rect = egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 8.0), egui::vec2(size.x - 16.0, 76.0));
        self.paint_thumbnail(ui, ctx, &video, thumb_rect);
        ui.painter().text(egui::pos2(rect.center().x, thumb_rect.max.y + 18.0), egui::Align2::CENTER_CENTER, &video.name, egui::TextStyle::Small.resolve(ui.style()), egui::Color32::from_gray(230));
        ui.painter().text(egui::pos2(rect.center().x, thumb_rect.max.y + 36.0), egui::Align2::CENTER_CENTER, format_duration(video.duration), egui::TextStyle::Small.resolve(ui.style()), egui::Color32::from_gray(165));
        if response.clicked() { self.select_video(idx); }
        ui.add_space(6.0);
    }

    fn ensure_thumbnail(&mut self, ctx: &egui::Context, video: &VideoMeta) -> Option<egui::TextureHandle> {
        let key = video.hash.clone();
        if !self.thumbnails.contains_key(&key) && !self.thumbnail_errors.contains_key(&key) {
            match crate::core::cache_manager::extract_thumbnail(&video.path, &video.hash, 0.0) {
                Ok(path) => match load_texture_from_path(ctx, &path, &format!("thumb_{key}")) { Ok(texture) => { self.thumbnails.insert(key.clone(), texture); } Err(err) => { self.thumbnail_errors.insert(key.clone(), err); } },
                Err(err) => { self.thumbnail_errors.insert(key.clone(), err.to_string()); }
            }
        }
        self.thumbnails.get(&key).cloned()
    }

    fn ensure_playback_frame(&mut self, ctx: &egui::Context, video: &VideoMeta) -> Option<egui::TextureHandle> {
        let frame_second = self.playback_position.floor().max(0.0);
        let key = format!("{}_{:.0}", video.hash, frame_second);
        if !self.playback_frames.contains_key(&key) && !self.playback_frame_errors.contains_key(&key) {
            match crate::core::cache_manager::extract_frames(&video.path, &video.hash, &[frame_second], self.settings.image_pixel_limit) {
                Ok(paths) => if let Some(path) = paths.first() { match load_texture_from_path(ctx, path, &format!("frame_{key}")) { Ok(texture) => { self.playback_frames.insert(key.clone(), texture); } Err(err) => { self.playback_frame_errors.insert(key.clone(), err); } } },
                Err(err) => { self.playback_frame_errors.insert(key.clone(), err.to_string()); }
            }
        }
        self.playback_frames.get(&key).cloned()
    }

    fn paint_thumbnail(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, video: &VideoMeta, rect: egui::Rect) {
        if let Some(texture) = self.ensure_thumbnail(ctx, video) { paint_texture_fit(ui, &texture, rect); } else { ui.painter().rect_filled(rect, 4.0, egui::Color32::BLACK); ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "缩略图", egui::TextStyle::Small.resolve(ui.style()), egui::Color32::from_gray(170)); }
    }

    fn paint_playback_frame(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, video: &VideoMeta, rect: egui::Rect) {
        let frame_texture = self.ensure_playback_frame(ctx, video);
        let fallback_texture = if frame_texture.is_some() { None } else { self.ensure_thumbnail(ctx, video) };
        if let Some(texture) = frame_texture.or(fallback_texture) { paint_texture_fit(ui, &texture, rect); } else { ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "视频播放预览区", egui::TextStyle::Heading.resolve(ui.style()), egui::Color32::from_gray(210)); }
    }

    fn settings_nav_button(&mut self, ui: &mut egui::Ui, section: SettingsSection, label: &str) { if ui.add_sized([ui.available_width(), 28.0], egui::SelectableLabel::new(self.settings_section == section, label)).clicked() { self.settings_section = section; } }
    fn hydrate_video_maybe(&mut self, idx: usize) {
        if idx >= self.videos.len() { return; }
        if self.videos[idx].duration > 0.0 { return; }
        if let Ok(h) = hydrate_video_meta(&self.videos[idx]) { self.videos[idx] = h; }
    }
    fn filtered_video_indices(&self) -> Vec<usize> { let q = self.search_query.trim().to_ascii_lowercase(); self.videos.iter().enumerate().filter_map(|(idx, video)| if q.is_empty() || video.name.to_ascii_lowercase().contains(&q) { Some(idx) } else { None }).collect() }
    fn current_video(&self) -> Option<&VideoMeta> { self.selected_index.and_then(|idx| self.videos.get(idx)) }

    fn select_video(&mut self, idx: usize) { self.hydrate_video_maybe(idx); self.selected_index = Some(idx); self.playback_position = 0.0; self.playback_playing = false; self.playback_last_tick = None; }
    fn switch_mode(&mut self, mode: ScreenMode) { self.mode = mode; if self.mode == ScreenMode::Overview { self.bottom_panel_height = self.bottom_panel_height.min(120.0).max(64.0); } else if matches!(self.mode, ScreenMode::Playback | ScreenMode::AiAnalysis) && self.bottom_panel_height < 180.0 { self.bottom_panel_height = 240.0; } }

    fn open_folder(&mut self, folder: PathBuf) {
        self.settings.last_folder = Some(folder.to_string_lossy().to_string());
        let _ = config::save_settings(&self.settings);
        self.folder = Some(folder.clone());
        self.scan_status = format!("正在扫描：{}", folder.display());
        self.thumbnails.clear(); self.thumbnail_errors.clear(); self.playback_frames.clear(); self.playback_frame_errors.clear(); self.search_results.clear();
        self.ai_status.set("程序正在扫描文件夹");
        if matches!(self.settings.cache_switch_policy, CacheSwitchPolicy::ClearOnFolderChange) { let _ = crate::core::cache_manager::clear_cache(&default_cache_root()); }
        match scan_videos(&folder.to_string_lossy()) { Ok(videos) => { self.videos = videos; self.selected_index = None; self.scan_status = format!("扫描完成：{} 个视频（缩略图懒加载）", self.videos.len()); self.persist_scanned_videos(); self.switch_mode(ScreenMode::Overview); self.ai_status.set("文件夹扫描完成，等待用户操作"); } Err(err) => { self.scan_status = format!("扫描失败：{err}"); self.ai_status.set("文件夹扫描失败"); } }
    }

    fn tick_playback(&mut self, ctx: &egui::Context) {
        if !self.playback_playing { self.playback_last_tick = None; return; }
        let now = Instant::now();
        if let Some(last) = self.playback_last_tick { self.playback_position += now.saturating_duration_since(last).as_secs_f64(); }
        self.playback_last_tick = Some(now);
        if let Some(video) = self.current_video() { if video.duration > 0.0 && self.playback_position >= video.duration { self.playback_position = video.duration; self.playback_playing = false; } }
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn persist_scanned_videos(&mut self) { if let Ok(db) = Database::open(&self.db_path) { for video in &self.videos { let _ = db.upsert_video(video); } } }
    fn search_current_database(&mut self) { match Database::open(&self.db_path).and_then(|db| db.search(&self.search_query, 50)) { Ok(results) => self.search_results = results, Err(err) => self.chat_log.push(format!("搜索失败：{err}")), } }
    fn generate_visible_thumbnails(&mut self) { self.chat_log.push("系统：缩略图已改为懒加载。滚动到哪里就生成哪里附近的缩略图。".to_string()); self.ai_status.set("缩略图懒加载已启用"); }

    fn start_analysis_queue(&mut self) {
        if self.ai_running { return; }
        if self.videos.is_empty() { self.chat_log.push("系统：没有可分析的视频。".to_string()); return; }
        self.debug_log.push(format!("启动分析队列，共 {} 个视频", self.videos.len().saturating_sub(self.selected_index.unwrap_or(0))));
        let start = self.selected_index.unwrap_or(0);
        self.selected_index = Some(start);
        let videos: Vec<VideoMeta> = self.videos.iter().skip(start).cloned().collect();
        let settings = self.settings.clone();
        let db_path = self.db_path.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_thread = cancel.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let settings = settings;
            let _ = tx.send(AiEvent::Status("程序启动 AI 分析队列".to_string()));
            let _ = tx.send(AiEvent::Debug("AI 分析队列已启动".to_string()));
            let _ = tx.send(AiEvent::Message(format!("系统：从第 {} 个视频开始分析。", start + 1)));
            for (offset, video) in videos.into_iter().enumerate() {
                if cancel_thread.load(Ordering::Relaxed) { let _ = tx.send(AiEvent::Message("系统：队列已暂停。".to_string())); break; }
                let index = start + offset;
                let _ = tx.send(AiEvent::Select(index));
                let _ = tx.send(AiEvent::Debug(format!("开始分析: {} (hash={}, duration={:.3}s)", video.name, video.hash, video.duration)));
                let _ = tx.send(AiEvent::Message(format!("系统：开始分析 {}", video.name)));
                let hydrated = hydrate_video_meta(&video).unwrap_or(video.clone());
                if hydrated.duration <= 0.0 { let _ = tx.send(AiEvent::Error { index, name: hydrated.name.clone(), error: "无法读取视频时长，跳过".to_string() }); continue; }
                let min_frames = settings.max_images.min(3).max(1);
                if hydrated.duration < min_frames as f64 * 0.5 { let _ = tx.send(AiEvent::Error { index, name: hydrated.name.clone(), error: format!("视频时长 {:.2}s 太短，跳过", hydrated.duration) }); continue; }
                match analyze_video_with_events(&hydrated, &settings, &db_path, |s| { let _ = tx.send(AiEvent::Status(s.to_string())); }, |_| {}) {
                    Ok(result) => {
                        let _ = tx.send(AiEvent::Debug(format!("分析完成: {}", hydrated.name)));
                        let _ = tx.send(AiEvent::Completed { index, name: hydrated.name.clone(), result });
                    }
                    Err(err) => {
                        let _ = tx.send(AiEvent::Debug(format!("分析失败: {} - {}", hydrated.name, err)));
                        let _ = tx.send(AiEvent::Error { index, name: hydrated.name.clone(), error: err.to_string() });
                    }
                }
            }
            let _ = tx.send(AiEvent::Finished);
        });
        self.ai_rx = Some(rx); self.ai_cancel = Some(cancel); self.ai_running = true; self.ai_paused = false; self.ai_status.set("程序启动 AI 分析队列");
    }

    fn pause_analysis_queue(&mut self) { if let Some(cancel) = &self.ai_cancel { cancel.store(true, Ordering::Relaxed); } self.ai_paused = true; self.chat_log.push("系统：已请求暂停。当前视频处理完成后停止队列。".to_string()); self.ai_status.set("程序已请求暂停 AI 分析队列"); }

    fn poll_ai_events(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.ai_rx.take() else { return; };
        let mut keep = true;
        while let Ok(event) = rx.try_recv() {
            match event {
                AiEvent::Status(status) => {
                    if let Some(debug) = status.strip_prefix("debug:") {
                        if self.settings.debug_mode { self.debug_log.push(debug.to_string()); }
                    } else {
                        self.ai_status.set(status);
                    }
                }
                AiEvent::Debug(msg) => { if self.settings.debug_mode { self.debug_log.push(msg); } }
                AiEvent::Message(msg) => self.chat_log.push(msg),
                AiEvent::Select(idx) => self.selected_index = Some(idx),
                AiEvent::Completed { index, name, result } => { if let Some(v) = self.videos.get_mut(index) { if v.duration <= 0.0 { if let Ok(h) = hydrate_video_meta(v) { *v = h; } } } self.ai_status.set(format!("AI 完成视频分析：{}", name)); self.chat_log.push(format!("AI：已完成 {}\n{}", name, render_analysis_text(&result))); }
                AiEvent::Error { index: _, name, error } => { self.ai_status.set(format!("AI 分析失败：{}", name)); self.chat_log.push(format!("AI：分析 {} 失败：{}", name, error)); }
                AiEvent::Finished => { self.ai_running = false; self.ai_paused = false; self.ai_cancel = None; keep = false; self.chat_log.push("系统：AI 队列已结束。".to_string()); self.ai_status.set("AI 队列已结束"); }
            }
            ctx.request_repaint();
        }
        if keep { self.ai_rx = Some(rx); }
    }

    fn send_current_question(&mut self) {
        let question = self.user_question.trim().to_string();
        self.user_question.clear();
        self.chat_log.push(format!("用户：{}", question));
        self.debug_log.push(format!("用户消息: {}", question));
        self.ai_status.set(STATUS_AI_RECEIVED_USER_MESSAGE);
        let Some(video) = self.current_video().cloned() else { self.chat_log.push("系统：未选择视频。".to_string()); return; };
        let settings = self.settings.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(QaEvent::Status(STATUS_AI_RECEIVED_USER_MESSAGE.to_string()));
            let hydrated = hydrate_video_meta(&video).unwrap_or(video);
            let _ = tx.send(QaEvent::Status("程序发送视频时长和限制文本".to_string()));
            let _ = tx.send(QaEvent::Debug(format!("视频: {} (时长 {:.3}s, {}x{} @ {:.1}fps)", hydrated.name, hydrated.duration, hydrated.width, hydrated.height, hydrated.fps)));
            let _ = tx.send(QaEvent::Status(STATUS_AI_ANALYZING.to_string()));
            let answer = ask_video_question_with_events(
                &hydrated,
                &question,
                &settings,
                |status| { let _ = tx.send(QaEvent::Status(localize_ai_status(status))); },
                |delta| { let _ = tx.send(QaEvent::Delta(delta.to_string())); },
            );
            match answer {
                Ok(answer) => {
                    let _ = tx.send(QaEvent::Debug(format!("AI 回答 ({} 字符): {}", answer.chars().count(), if answer.len() > 200 { format!("{}...", &answer[..200]) } else { answer.clone() })));
                    let _ = tx.send(QaEvent::Answer(answer));
                }
                Err(err) => {
                    let _ = tx.send(QaEvent::Debug(format!("AI 回答失败: {err}")));
                    let _ = tx.send(QaEvent::Error(format!("AI 问答失败：{}", err)));
                }
            }
            let _ = tx.send(QaEvent::Finished);
        });
        self.qa_rx = Some(rx); self.qa_running = true;
        self.qa_stream_index = None;
    }

    fn poll_qa_events(&mut self, ctx: &egui::Context) {
        if let Some(rx) = self.qa_rx.take() {
            let mut keep = true;
            while let Ok(event) = rx.try_recv() {
                match event {
                    QaEvent::Status(status) => {
                        if let Some(debug) = status.strip_prefix("debug:") {
                            if self.settings.debug_mode { self.debug_log.push(debug.to_string()); }
                        } else {
                            self.ai_status.set(status);
                        }
                    }
                    QaEvent::Debug(msg) => { if self.settings.debug_mode { self.debug_log.push(msg); } }
                    QaEvent::Delta(delta) => {
                        self.ai_status.set(STATUS_AI_ANSWERING);
                        if self.qa_stream_index.is_none() { self.chat_log.push("AI：".to_string()); self.qa_stream_index = Some(self.chat_log.len() - 1); }
                        if let Some(idx) = self.qa_stream_index { if let Some(line) = self.chat_log.get_mut(idx) { line.push_str(&delta); } }
                    }
                    QaEvent::Answer(answer) => {
                        if self.qa_stream_index.is_none() { self.chat_log.push(format!("AI：{}", answer)); }
                    }
                    QaEvent::Error(error) => { self.chat_log.push(format!("AI：{}", error)); self.ai_status.set("AI 问答失败"); }
                    QaEvent::Finished => { self.qa_running = false; keep = false; self.qa_stream_index = None; self.ai_status.set(STATUS_AI_ANSWER_COMPLETE); }
                }
                ctx.request_repaint();
            }
            if keep { self.qa_rx = Some(rx); }
        }
    }

    fn reset_chat_context(&mut self) {
        self.chat_log.clear();
        self.chat_log.push("系统：已刷新对话，上下文已清空。".to_string());
        self.user_question.clear();
        self.qa_rx = None;
        self.qa_running = false;
        self.qa_stream_index = None;
        self.ai_status = AiStatusState::default();
        self.ai_status.set("对话上下文已刷新");
        self.debug_log.clear();
        self.debug_follow = true;
        self.debug_follow_blocked_until = None;
    }

    fn show_debug_window(&mut self, ctx: &egui::Context) {
        if !self.settings.debug_mode { return; }
        if self.debug_log.len() > 1000 {
            self.debug_log.drain(0..(self.debug_log.len() - 1000));
        }
        let now = Instant::now();
        if let Some(until) = self.debug_follow_blocked_until {
            if now >= until { self.debug_follow = true; self.debug_follow_blocked_until = None; }
        }
        let window_response = egui::Window::new("🧪 调试日志").resizable(true).default_size([480.0, 360.0]).min_width(280.0).min_height(160.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} 条", self.debug_log.len()));
                if ui.button("清空").clicked() { self.debug_log.clear(); }
                if ui.button(if self.debug_follow { "跟随中" } else { "暂停跟随" }).clicked() { self.debug_follow = !self.debug_follow; if !self.debug_follow { self.debug_follow_blocked_until = Some(now + Duration::from_secs(10)); } }
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for line in &self.debug_log {
                    let display = if line.starts_with("🖼") || line.starts_with("🎧") {
                        line.to_string()
                    } else {
                        format!("  {}", line)
                    };
                    ui.label(display);
                }
                if self.debug_follow {
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                }
            });
        });
        if let Some(window_response) = window_response {
            let scrolled = ctx.input(|i| i.raw_scroll_delta != egui::Vec2::ZERO);
            if window_response.response.hovered() && scrolled {
                if self.debug_follow { self.debug_follow = false; self.debug_follow_blocked_until = Some(now + Duration::from_secs(10)); }
            }
        }
    }

    fn refresh_model_scripts(&mut self) { self.model_scripts = models::list_model_scripts(); if self.selected_model_script.is_none() || self.selected_model_script.as_ref().is_some_and(|p| !p.exists()) { self.selected_model_script = self.model_scripts.first().cloned(); } self.model_status = format!("发现 {} 个启动文件。", self.model_scripts.len()); }
    fn start_selected_model(&mut self) { if self.model_child.is_some() { self.show_notice("模型已在运行", "本程序已经记录了一个模型进程，请先终止后再启动。".to_string()); return; } if self.has_pending_model_check(ModelCheckKind::Start) { self.show_notice("正在检测", "模型启动检测尚未完成，请不要重复点击启动。".to_string()); return; } if models::is_llama_service_ready(Duration::from_millis(250)) { self.model_status = "7080 服务已经可用，未重复启动脚本。".to_string(); self.show_notice("模型服务已可用", "检测到 127.0.0.1:7080 已经可连接，因此没有再次启动脚本。".to_string()); return; } let Some(script) = self.selected_model_script.clone() else { self.show_notice("未选择启动文件", format!("请将 .bat/.cmd 或 .sh 放入 {}，然后点击刷新。", models::ensure_models_dir().display())); return; }; self.settings.model_name = script.file_stem().and_then(|s| s.to_str()).unwrap_or("local-llamacpp").to_string(); match models::start_model_script(&script) { Ok(child) => { let pid = child.id(); self.model_child = Some(child); self.model_status = format!("已启动 PID {}，最多等待 10 秒检测 7080。", pid); self.pending_model_check = Some(PendingModelCheck { due: Instant::now() + Duration::from_secs(10), kind: ModelCheckKind::Start }); self.show_notice("已发送启动命令", format!("已启动脚本：{}\n进程 PID：{}\n程序会在 10 秒内持续检测 127.0.0.1:7080 是否可用。", script.display(), pid)); } Err(err) => { self.model_status = err.clone(); self.show_notice("启动失败", err); } } }
    fn stop_model(&mut self) { if let Some(mut child) = self.model_child.take() { let pid = child.id(); models::stop_model_process(&mut child); self.model_status = format!("已终止本程序记录的 PID {}，继续强杀 7080。", pid); } match models::kill_7080_processes() { Ok(()) => { self.model_status = "已发送 7080 端口强制终止命令，1 秒后检测端口。".to_string(); self.pending_model_check = Some(PendingModelCheck { due: Instant::now() + Duration::from_secs(1), kind: ModelCheckKind::Stop }); self.show_notice("已强制终止 7080", "已尝试杀死所有占用 127.0.0.1:7080 的进程。程序会在 1 秒后检测端口是否释放。".to_string()); } Err(err) => { self.model_status = err.clone(); self.show_notice("终止失败", err); } } }
    fn poll_model_process(&mut self) { if let Some(child) = self.model_child.as_mut() { if child.try_wait().ok().flatten().is_some() { self.model_child = None; self.model_status = "模型进程已退出。".to_string(); } } }
    fn poll_model_health(&mut self) { let Some(check) = self.pending_model_check.clone() else { return; }; let ready = models::is_llama_service_ready(Duration::from_millis(300)); match check.kind { ModelCheckKind::Start => { if ready { self.pending_model_check = None; self.model_status = "7080 服务已连接，模型运行正常。".to_string(); self.show_notice("模型启动成功", "已成功连接 127.0.0.1:7080。".to_string()); return; } if Instant::now() >= check.due { self.pending_model_check = None; self.model_status = "10 秒内未能连接 7080，请检查脚本和模型日志。".to_string(); self.show_notice("模型可能未就绪", "启动命令已发送，但 10 秒内无法连接 127.0.0.1:7080。模型可能仍在加载，或脚本启动失败。".to_string()); } } ModelCheckKind::Stop => { if Instant::now() < check.due { return; } self.pending_model_check = None; if ready { self.model_status = "7080 仍可连接，可再次点击终止。".to_string(); self.show_notice("7080 仍可连接", "强制终止后 7080 仍可连接。可以再次点击“终止”。".to_string()); } else { self.model_status = "7080 已释放。".to_string(); self.show_notice("7080 已释放", "检测到 127.0.0.1:7080 已不可连接。".to_string()); } } } }
    fn has_pending_model_check(&self, kind: ModelCheckKind) -> bool { self.pending_model_check.as_ref().map(|check| check.kind == kind).unwrap_or(false) }
    fn show_notice(&mut self, title: &str, body: String) {
        self.model_notice_title = title.to_string();
        self.model_notice_body = body;
        self.model_notice_open = true;
        self.model_notice_dismiss_at = Some(Instant::now() + Duration::from_secs(5));
    }
    fn show_model_notice(&mut self, ctx: &egui::Context) {
        if !self.model_notice_open { return; }
        if let Some(dismiss_at) = self.model_notice_dismiss_at {
            if Instant::now() >= dismiss_at { self.model_notice_open = false; return; }
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        let mut open = self.model_notice_open;
        let title = self.model_notice_title.clone();
        let body = self.model_notice_body.clone();
        egui::Window::new(&title).collapsible(false).resizable(true).default_width(380.0).open(&mut open).show(ctx, |ui| {
            ui.label(&body);
        });
        if !open { self.model_notice_open = false; }
    }
    fn save_settings_with_notice(&mut self) { match config::save_settings(&self.settings) { Ok(()) => self.show_notice("设置已保存", format!("保存到：{}", config::settings_path().display())), Err(err) => self.show_notice("保存设置失败", err.to_string()), } }
    fn clean_cache_to_limit(&mut self) { match enforce_cache_size_limit(&default_cache_root(), self.settings.cache_size_limit_mb) { Ok(bytes) => self.show_notice("缓存清理完成", format!("已删除约 {:.2} MB 缓存。", bytes as f64 / 1024.0 / 1024.0)), Err(err) => self.show_notice("缓存清理失败", err.to_string()), } }
}

impl Drop for AiVideoApp { fn drop(&mut self) { if let Some(cancel) = &self.ai_cancel { cancel.store(true, Ordering::Relaxed); } if let Some(mut child) = self.model_child.take() { models::stop_model_process(&mut child); } let _ = models::kill_7080_processes(); } }

fn prompt_button(ui: &mut egui::Ui, label: &str, file_name: &str, app: &mut AiVideoApp) { ui.horizontal(|ui| { if ui.button(label).clicked() { match prompts::open_prompt_file(file_name) { Ok(()) => app.show_notice("已打开提示词文档", format!("已打开：{}", prompts::prompt_path(file_name).display())), Err(err) => app.show_notice("打开失败", err), } } ui.small(elide_middle(&prompts::prompt_path(file_name).display().to_string(), 60)); }); }
fn load_texture_from_path(ctx: &egui::Context, path: &str, key: &str) -> Result<egui::TextureHandle, String> { let image = image::open(Path::new(path)).map_err(|err| err.to_string())?.to_rgba8(); let size = [image.width() as usize, image.height() as usize]; let pixels = image.into_raw(); let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels); Ok(ctx.load_texture(key.to_string(), color_image, egui::TextureOptions::LINEAR)) }
fn paint_texture_fit(ui: &egui::Ui, texture: &egui::TextureHandle, rect: egui::Rect) { ui.painter().rect_filled(rect, 4.0, egui::Color32::BLACK); let [tw, th] = texture.size(); if tw == 0 || th == 0 { return; } let texture_aspect = tw as f32 / th as f32; let rect_aspect = rect.width() / rect.height(); let draw_size = if texture_aspect > rect_aspect { egui::vec2(rect.width(), rect.width() / texture_aspect) } else { egui::vec2(rect.height() * texture_aspect, rect.height()) }; let draw_rect = egui::Rect::from_center_size(rect.center(), draw_size); ui.painter().image(texture.id(), draw_rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE); }
fn format_duration(seconds: f64) -> String { if seconds <= 0.0 { return "时长未知".to_string(); } let total = seconds.round() as u64; let mins = total / 60; let secs = total % 60; format!("{mins}:{secs:02}") }
fn elide_middle(text: &str, max_chars: usize) -> String { let count = text.chars().count(); if count <= max_chars { return text.to_string(); } if max_chars <= 3 { return "...".to_string(); } let keep = max_chars - 3; let left = keep / 2; let right = keep - left; let start: String = text.chars().take(left).collect(); let end: String = text.chars().rev().take(right).collect::<String>().chars().rev().collect(); format!("{start}...{end}") }
fn setting_text(ui: &mut egui::Ui, label: &str, value: &mut String) { ui.vertical(|ui| { ui.label(label); ui.add_sized([ui.available_width().min(560.0), 24.0], egui::TextEdit::singleline(value)); }); ui.add_space(12.0); }
fn setting_slider_usize(ui: &mut egui::Ui, label: &str, value: &mut usize, range: std::ops::RangeInclusive<usize>) { ui.vertical(|ui| { ui.label(label); ui.add_sized([ui.available_width().min(560.0), 24.0], egui::Slider::new(value, range)); }); ui.add_space(12.0); }
fn setting_slider_u32(ui: &mut egui::Ui, label: &str, value: &mut u32, range: std::ops::RangeInclusive<u32>) { ui.vertical(|ui| { ui.label(label); ui.add_sized([ui.available_width().min(560.0), 24.0], egui::Slider::new(value, range)); }); ui.add_space(12.0); }
fn setting_slider_u64(ui: &mut egui::Ui, label: &str, value: &mut u64, range: std::ops::RangeInclusive<u64>) { ui.vertical(|ui| { ui.label(label); ui.add_sized([ui.available_width().min(560.0), 24.0], egui::Slider::new(value, range)); }); ui.add_space(12.0); }
fn setting_slider_f32(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) { ui.vertical(|ui| { ui.label(label); ui.add_sized([ui.available_width().min(560.0), 24.0], egui::Slider::new(value, range)); }); ui.add_space(12.0); }
fn settings_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) { egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(14)).show(ui, |ui| { ui.set_width(ui.available_width()); ui.vertical(add_contents); }); }
fn localize_ai_status(status: &str) -> String { match status { "AI received user question" => STATUS_AI_RECEIVED_USER_MESSAGE.to_string(), "program sent video metadata and limits to AI" => "程序发送视频时长和限制文本".to_string(), "AI requested video evidence from program" => "AI 向程序请求视频片段".to_string(), "program prepared requested video evidence" => STATUS_PROGRAM_REQUEST_OK.to_string(), "program sent evidence to AI" => "程序将视频片段发送给 AI".to_string(), "AI answer complete" => STATUS_AI_ANSWER_COMPLETE.to_string(), "AI answered without requesting program evidence" => "AI 未请求片段，直接回答".to_string(), other => other.to_string() } }
fn default_db_path() -> PathBuf { dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("ai-video").join("ai-video.sqlite3") }
fn nav_button(ui: &mut egui::Ui, label: &str) -> egui::Response { ui.add_sized([ui.available_width(), 36.0], egui::Button::new(label)) }
fn panel_frame() -> egui::Frame { egui::Frame::default().fill(egui::Color32::from_gray(24)).stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(48))).inner_margin(egui::Margin::same(8)) }
fn content_frame() -> egui::Frame { egui::Frame::default().fill(egui::Color32::from_gray(18)).inner_margin(egui::Margin::same(12)) }
fn apply_gray_theme(ctx: &egui::Context) { let mut visuals = egui::Visuals::dark(); visuals.window_fill = egui::Color32::from_gray(22); visuals.panel_fill = egui::Color32::from_gray(24); visuals.extreme_bg_color = egui::Color32::from_gray(8); visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(55); visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(70); visuals.widgets.active.bg_fill = egui::Color32::from_gray(85); ctx.set_visuals(visuals); }
