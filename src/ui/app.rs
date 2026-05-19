use crate::core::settings::AppSettings;
use crate::core::video_manager::{scan_videos, VideoMeta};
use crate::db::{Database, SearchResult};
use crate::models;
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScreenMode { Overview, Playback, AiAnalysis, Settings }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsSection { Model, AiLimits, Media, Cache, Debug }

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
}

impl Default for AiVideoApp {
    fn default() -> Self {
        let scripts = models::list_model_scripts();
        let selected = scripts.first().cloned();
        Self {
            mode: ScreenMode::Overview,
            settings_section: SettingsSection::Model,
            settings: AppSettings::default(),
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
            thumbnails: HashMap::new(),
            thumbnail_errors: HashMap::new(),
            playback_position: 0.0,
            playback_playing: false,
            playback_last_tick: None,
            playback_frames: HashMap::new(),
            playback_frame_errors: HashMap::new(),
            model_scripts: scripts,
            selected_model_script: selected,
            model_child: None,
            model_status: format!("模型目录：{}", models::ensure_models_dir().display()),
        }
    }
}

impl eframe::App for AiVideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_gray_theme(ctx);
        self.tick_playback(ctx);
        self.poll_model_process();

        egui::TopBottomPanel::top("top_bar").frame(panel_frame()).show(ctx, |ui| self.top_bar(ui));
        egui::SidePanel::left("control_panel")
            .resizable(true)
            .default_width(280.0)
            .width_range(120.0..=650.0)
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
                .resizable(true)
                .default_height(if self.mode == ScreenMode::Overview { 86.0 } else { 240.0 })
                .height_range(36.0..=720.0)
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
            ui.label(match self.mode {
                ScreenMode::Overview => "总览模式",
                ScreenMode::Playback => "播放模式",
                ScreenMode::AiAnalysis => "AI 分析模式",
                ScreenMode::Settings => "设置",
            });
            ui.separator();
            ui.label(&self.scan_status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label("FFmpeg / mpv 外部播放"));
        });
    }

    fn left_control_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("控制面板");
        ui.separator();
        if nav_button(ui, "选择文件夹").clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                self.folder = Some(folder.clone());
                self.scan_folder(folder);
                self.mode = ScreenMode::Overview;
            }
        }
        if nav_button(ui, "分析文件/生成缩略图").clicked() {
            self.generate_visible_thumbnails();
        }

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new("模型启动").strong());
            ui.small(format!("放置目录：{}", models::ensure_models_dir().display()));
            ui.horizontal(|ui| {
                if ui.button("刷新").clicked() { self.refresh_model_scripts(); }
                if ui.button("启动").clicked() { self.start_selected_model(); }
                if ui.button("停止").clicked() { self.stop_model(); }
            });
            egui::ComboBox::from_id_salt("model_script_combo")
                .selected_text(self.selected_model_script.as_ref().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("未选择启动文件"))
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for path in self.model_scripts.clone() {
                        let label = path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
                        ui.selectable_value(&mut self.selected_model_script, Some(path), label);
                    }
                });
            ui.small(&self.model_status);
        });

        ui.add_space(8.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new("当前目录").strong());
            ui.small(self.folder.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "未选择".to_string()));
        });

        ui.add_space(8.0);
        if nav_button(ui, "媒体总览").clicked() { self.mode = ScreenMode::Overview; }
        if nav_button(ui, "进入播放").clicked() {
            if self.selected_index.is_none() && !self.videos.is_empty() { self.select_video(0); }
            self.mode = ScreenMode::Playback;
        }
        if nav_button(ui, if self.mode == ScreenMode::AiAnalysis { "退出 AI 分析" } else { "AI 分析" }).clicked() {
            if self.mode == ScreenMode::AiAnalysis { self.mode = ScreenMode::Overview; }
            else {
                if self.selected_index.is_none() && !self.videos.is_empty() { self.select_video(0); }
                self.mode = ScreenMode::AiAnalysis;
                self.chat_log.push("系统：已切换到 AI 分析模式。".to_string());
            }
        }
        if nav_button(ui, "设置").clicked() { self.mode = ScreenMode::Settings; }

        ui.add_space(10.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new("目录信息").strong());
            ui.label(format!("视频数：{}", self.videos.len()));
            if let Some(idx) = self.selected_index.and_then(|i| self.videos.get(i).map(|_| i)) {
                ui.small(format!("当前：{}", self.videos[idx].name));
            }
        });
    }

    fn overview_grid(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("媒体总览");
            ui.separator();
            ui.label(format!("{} 个视频", self.filtered_video_indices().len()));
        });
        if !self.search_results.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("简介搜索结果").strong());
            let results = self.search_results.clone();
            for result in results.iter().take(8) {
                if ui.button(format!("{} - {}", result.name, result.title.clone().unwrap_or_default())).clicked() {
                    if let Some(idx) = self.videos.iter().position(|v| v.path == result.path) {
                        self.select_video(idx);
                        self.mode = ScreenMode::Playback;
                    }
                }
                if let Some(summary) = &result.summary { ui.small(summary); }
            }
            ui.separator();
        }
        ui.add_space(12.0);
        if self.videos.is_empty() {
            ui.centered_and_justified(|ui| ui.label("点击左侧“选择文件夹”后，视频会以卡片网格排列在这里。点击某个视频进入播放界面。"));
            return;
        }

        let card_width = 260.0;
        let card_height = 245.0;
        let spacing = 16.0;
        let cols = ((ui.available_width() + spacing) / (card_width + spacing)).floor().max(1.0) as usize;
        let indices = self.filtered_video_indices();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for row in indices.chunks(cols) {
                ui.horizontal_top(|ui| {
                    for &idx in row {
                        self.overview_video_card(ui, ctx, idx, egui::vec2(card_width, card_height));
                        ui.add_space(spacing);
                    }
                });
                ui.add_space(14.0);
            }
        });
    }

    fn overview_video_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, idx: usize, size: egui::Vec2) {
        let selected = self.selected_index == Some(idx);
        let video = self.videos[idx].clone();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let fill = if selected { egui::Color32::from_gray(44) } else { egui::Color32::from_gray(26) };
        ui.painter().rect_filled(rect, 6.0, fill);
        ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.0, egui::Color32::from_gray(if selected { 95 } else { 42 })), egui::StrokeKind::Outside);
        let thumb_rect = egui::Rect::from_min_size(rect.min + egui::vec2(10.0, 10.0), egui::vec2(size.x - 20.0, 170.0));
        self.paint_thumbnail(ui, ctx, &video, thumb_rect);
        ui.painter().text(egui::pos2(rect.center().x, thumb_rect.max.y + 20.0), egui::Align2::CENTER_CENTER, &video.name, egui::TextStyle::Button.resolve(ui.style()), egui::Color32::from_gray(235));
        ui.painter().text(egui::pos2(rect.center().x, thumb_rect.max.y + 42.0), egui::Align2::CENTER_CENTER, format_duration(video.duration), egui::TextStyle::Small.resolve(ui.style()), egui::Color32::from_gray(165));
        if response.clicked() {
            self.select_video(idx);
            self.mode = ScreenMode::Playback;
        }
    }

    fn playback_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("视频播放界面");
            ui.separator();
            if ui.button("返回媒体总览").clicked() { self.mode = ScreenMode::Overview; }
            if ui.button("切到 AI 分析").clicked() { self.mode = ScreenMode::AiAnalysis; }
        });
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
                if ui.button("开始/恢复队列").clicked() {
                    self.ai_running = true;
                    self.ai_paused = false;
                    if let Some((video_name, duration)) = self.current_video().map(|video| (video.name.clone(), video.duration)) {
                        self.chat_log.push(format!("系统：开始分析 {}", video_name));
                        self.chat_log.push(format!("AI：已接收视频总时长 {}、最大图片数 {}、最大音频段数 {}。", format_duration(duration), self.settings.max_images, self.settings.max_audio_segments));
                    } else {
                        self.chat_log.push("系统：没有可分析的视频。".to_string());
                    }
                }
                if ui.button("暂停队列，允许提问").clicked() && self.ai_running {
                    self.ai_paused = true;
                    self.chat_log.push("系统：后台分析已暂停，现在允许用户针对当前视频提问。".to_string());
                }
            });
            let state = if self.ai_running && self.ai_paused { "已暂停，可提问" } else if self.ai_running { "后台分析中，禁止提问" } else { "未开始" };
            ui.label(format!("当前状态：{state}"));
        });
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.separator();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(210.0);
                ui.label(egui::RichText::new("设置导航").strong());
                self.settings_nav_button(ui, SettingsSection::Model, "模型启动");
                self.settings_nav_button(ui, SettingsSection::AiLimits, "AI 限制");
                self.settings_nav_button(ui, SettingsSection::Media, "媒体处理");
                self.settings_nav_button(ui, SettingsSection::Cache, "缓存策略");
                self.settings_nav_button(ui, SettingsSection::Debug, "调试模式");
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_min_width(420.0);
                match self.settings_section {
                    SettingsSection::Model => {
                        ui.heading("模型启动");
                        ui.label(format!("将 llama.cpp 启动脚本放到：{}", models::ensure_models_dir().display()));
                        ui.horizontal(|ui| {
                            ui.label("接口");
                            ui.text_edit_singleline(&mut self.settings.llama_cpp_endpoint);
                        });
                        ui.horizontal(|ui| {
                            ui.label("模型名");
                            ui.text_edit_singleline(&mut self.settings.model_name);
                        });
                        if ui.button("刷新启动文件列表").clicked() { self.refresh_model_scripts(); }
                        ui.label(&self.model_status);
                    }
                    SettingsSection::AiLimits => {
                        ui.heading("AI 限制");
                        ui.add(egui::Slider::new(&mut self.settings.max_images, 1..=64).text("第一次分析发送图片数 / 请求图片上限"));
                        ui.add(egui::Slider::new(&mut self.settings.max_audio_segments, 0..=32).text("音频段数上限"));
                        ui.add(egui::Slider::new(&mut self.settings.max_context_tokens, 1024..=65536).text("最大上下文 token 长度"));
                    }
                    SettingsSection::Media => {
                        ui.heading("媒体处理");
                        ui.add(egui::Slider::new(&mut self.settings.audio_clip_seconds, 1.0..=30.0).text("音频截取长度/s"));
                        ui.add(egui::Slider::new(&mut self.settings.image_pixel_limit, 1000..=100000).text("图片压缩总像素上限"));
                        ui.add(egui::Slider::new(&mut self.settings.audio_sample_rate, 8000..=48000).text("音频采样率"));
                    }
                    SettingsSection::Cache => {
                        ui.heading("缓存策略");
                        ui.add(egui::Slider::new(&mut self.settings.cache_size_limit_mb, 128..=65536).text("缓存大小上限 / MB"));
                        ui.label("缓存目录结构：cache/thumbs、cache/frames、cache/audio");
                        ui.label("后续将按大小清理最旧缓存。切换文件夹策略由 cache_switch_policy 控制。");
                    }
                    SettingsSection::Debug => {
                        ui.heading("调试模式");
                        ui.checkbox(&mut self.settings.debug_mode, "显示原始 JSON");
                        ui.label("非调试模式下，聊天栏仅显示清洗后的文本。");
                    }
                }
            });
        });
    }

    fn right_ai_video_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("AI 队列");
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for idx in 0..self.videos.len() { self.side_video_card(ui, ctx, idx); }
        });
    }

    fn bottom_input_area(&mut self, ui: &mut egui::Ui) {
        if self.mode == ScreenMode::Overview {
            ui.horizontal(|ui| {
                ui.label("搜索");
                let input_width = (ui.available_width() - 90.0).max(120.0);
                ui.add_sized([input_width, 26.0], egui::TextEdit::singleline(&mut self.search_query).hint_text("搜索文件名或已生成的 AI 简介"));
                if ui.button("搜索").clicked() { self.search_current_database(); }
            });
            return;
        }
        self.bottom_chat(ui);
    }

    fn bottom_chat(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("AI 对话");
            ui.separator();
            let prompt_state = if self.mode == ScreenMode::AiAnalysis && self.ai_running && !self.ai_paused { "后台分析中：输入禁用" } else { "可显示程序与 AI 消息" };
            ui.label(prompt_state);
        });
        let log_height = (ui.available_height() - 34.0).max(50.0);
        egui::ScrollArea::vertical().max_height(log_height).auto_shrink([false, false]).show(ui, |ui| {
            let keep_from = self.chat_log.len().saturating_sub(80);
            for line in &self.chat_log[keep_from..] {
                egui::Frame::group(ui.style()).show(ui, |ui| { ui.label(line); });
                ui.add_space(4.0);
            }
        });
        ui.horizontal(|ui| {
            let disabled = self.mode == ScreenMode::AiAnalysis && self.ai_running && !self.ai_paused;
            ui.add_enabled_ui(!disabled, |ui| {
                ui.text_edit_singleline(&mut self.user_question);
                if ui.button("发送").clicked() && !self.user_question.trim().is_empty() {
                    self.chat_log.push(format!("用户：{}", self.user_question.trim()));
                    self.chat_log.push("AI：当前对话入口已保留，后续接入本地模型问答。".to_string());
                    self.user_question.clear();
                }
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
                if ui.button(button_text).clicked() {
                    self.playback_playing = !self.playback_playing;
                    self.playback_last_tick = Some(Instant::now());
                }
                if ui.button("停止").clicked() {
                    self.playback_playing = false;
                    self.playback_position = 0.0;
                    self.playback_last_tick = None;
                }
                if ui.button("用 mpv 播放").clicked() {
                    match crate::core::player::open_with_mpv(&video.path, self.playback_position) {
                        Ok(()) => self.chat_log.push(format!("系统：已调用 mpv 播放 {}", video.name)),
                        Err(err) => self.chat_log.push(format!("系统：mpv 启动失败：{err}")),
                    }
                }
                let duration = video.duration.max(0.1);
                let pos_label = format!("{} / {}", format_duration(self.playback_position), format_duration(video.duration));
                let slider = egui::Slider::new(&mut self.playback_position, 0.0..=duration).show_value(false).text(pos_label);
                if ui.add(slider).changed() { self.playback_last_tick = Some(Instant::now()); }
            });
            ui.heading(&video.name);
            ui.label(format_duration(video.duration));
            ui.small("说明：当前内置区域仍是抽帧预览；完整播放请用 mpv 外部窗口。后续再接 libmpv 内嵌。 ");
        } else {
            ui.centered_and_justified(|ui| ui.label(empty_hint));
        }
    }

    fn current_summary_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(video) = self.current_video() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new("AI 简介").strong());
                match Database::open(&self.db_path).and_then(|db| db.get_summary_by_hash(&video.hash)) {
                    Ok(Some(summary)) => ui.label(crate::ai::render_analysis_text(&summary)),
                    Ok(None) => ui.label("当前视频尚未生成简介。"),
                    Err(err) => ui.label(format!("读取简介失败：{err}")),
                };
            });
        }
    }

    fn side_video_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, idx: usize) {
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
                Ok(path) => match load_texture_from_path(ctx, &path, &format!("thumb_{key}")) {
                    Ok(texture) => { self.thumbnails.insert(key.clone(), texture); }
                    Err(err) => { self.thumbnail_errors.insert(key.clone(), err); }
                },
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
                Ok(paths) => if let Some(path) = paths.first() {
                    match load_texture_from_path(ctx, path, &format!("frame_{key}")) {
                        Ok(texture) => { self.playback_frames.insert(key.clone(), texture); }
                        Err(err) => { self.playback_frame_errors.insert(key.clone(), err); }
                    }
                },
                Err(err) => { self.playback_frame_errors.insert(key.clone(), err.to_string()); }
            }
        }
        self.playback_frames.get(&key).cloned()
    }

    fn paint_thumbnail(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, video: &VideoMeta, rect: egui::Rect) {
        if let Some(texture) = self.ensure_thumbnail(ctx, video) { paint_texture_fit(ui, &texture, rect); }
        else {
            ui.painter().rect_filled(rect, 4.0, egui::Color32::BLACK);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "缩略图", egui::TextStyle::Small.resolve(ui.style()), egui::Color32::from_gray(170));
        }
    }

    fn paint_playback_frame(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, video: &VideoMeta, rect: egui::Rect) {
        let frame_texture = self.ensure_playback_frame(ctx, video);
        let fallback_texture = if frame_texture.is_some() { None } else { self.ensure_thumbnail(ctx, video) };
        if let Some(texture) = frame_texture.or(fallback_texture) { paint_texture_fit(ui, &texture, rect); }
        else { ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "视频播放预览区", egui::TextStyle::Heading.resolve(ui.style()), egui::Color32::from_gray(210)); }
    }

    fn settings_nav_button(&mut self, ui: &mut egui::Ui, section: SettingsSection, label: &str) {
        if ui.selectable_label(self.settings_section == section, label).clicked() { self.settings_section = section; }
    }

    fn filtered_video_indices(&self) -> Vec<usize> {
        let query = self.search_query.trim().to_ascii_lowercase();
        self.videos.iter().enumerate().filter_map(|(idx, video)| {
            if query.is_empty() || video.name.to_ascii_lowercase().contains(&query) { Some(idx) } else { None }
        }).collect()
    }

    fn select_video(&mut self, idx: usize) {
        if self.selected_index != Some(idx) {
            self.selected_index = Some(idx);
            self.playback_position = 0.0;
            self.playback_playing = false;
            self.playback_last_tick = None;
        } else { self.selected_index = Some(idx); }
    }

    fn tick_playback(&mut self, ctx: &egui::Context) {
        if !self.playback_playing { self.playback_last_tick = None; return; }
        let now = Instant::now();
        if let Some(last) = self.playback_last_tick { self.playback_position += now.saturating_duration_since(last).as_secs_f64(); }
        self.playback_last_tick = Some(now);
        if let Some(video) = self.current_video() {
            if video.duration > 0.0 && self.playback_position >= video.duration {
                self.playback_position = video.duration;
                self.playback_playing = false;
            }
        }
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn scan_folder(&mut self, folder: PathBuf) {
        self.scan_status = format!("正在扫描：{}", folder.display());
        self.thumbnails.clear();
        self.thumbnail_errors.clear();
        self.playback_frames.clear();
        self.playback_frame_errors.clear();
        match scan_videos(&folder.to_string_lossy()) {
            Ok(videos) => {
                self.videos = videos;
                self.selected_index = None;
                self.playback_position = 0.0;
                self.playback_playing = false;
                self.scan_status = format!("扫描完成：{} 个视频", self.videos.len());
                self.persist_scanned_videos();
            }
            Err(err) => { self.scan_status = format!("扫描失败：{err}"); }
        }
    }

    fn persist_scanned_videos(&mut self) {
        match Database::open(&self.db_path) {
            Ok(db) => { for video in &self.videos { let _ = db.upsert_video(video); } }
            Err(err) => self.chat_log.push(format!("数据库初始化失败：{err}")),
        }
    }

    fn search_current_database(&mut self) {
        match Database::open(&self.db_path).and_then(|db| db.search(&self.search_query, 50)) {
            Ok(results) => self.search_results = results,
            Err(err) => self.chat_log.push(format!("搜索失败：{err}")),
        }
    }

    fn generate_visible_thumbnails(&mut self) {
        let count = self.videos.len();
        for video in &self.videos {
            let _ = crate::core::cache_manager::extract_thumbnail(&video.path, &video.hash, 0.0);
        }
        self.thumbnails.clear();
        self.thumbnail_errors.clear();
        self.chat_log.push(format!("系统：已尝试生成 {} 个视频缩略图。", count));
    }

    fn refresh_model_scripts(&mut self) {
        self.model_scripts = models::list_model_scripts();
        if self.selected_model_script.is_none() || self.selected_model_script.as_ref().is_some_and(|p| !p.exists()) {
            self.selected_model_script = self.model_scripts.first().cloned();
        }
        self.model_status = format!("发现 {} 个启动文件。", self.model_scripts.len());
    }

    fn start_selected_model(&mut self) {
        if self.model_child.is_some() {
            self.model_status = "模型进程已在运行。".to_string();
            return;
        }
        let Some(script) = self.selected_model_script.clone() else {
            self.model_status = format!("未选择启动文件。请将 .bat/.cmd 或 .sh 放入 {}", models::ensure_models_dir().display());
            return;
        };
        self.settings.model_name = script.file_stem().and_then(|s| s.to_str()).unwrap_or("local-llamacpp").to_string();
        match models::start_model_script(&script) {
            Ok(child) => {
                self.model_child = Some(child);
                self.model_status = format!("已启动：{}", script.display());
            }
            Err(err) => self.model_status = err,
        }
    }

    fn stop_model(&mut self) {
        if let Some(mut child) = self.model_child.take() {
            models::stop_model_process(&mut child);
            self.model_status = "模型进程已停止。".to_string();
        } else { self.model_status = "没有正在运行的模型进程。".to_string(); }
    }

    fn poll_model_process(&mut self) {
        if let Some(child) = self.model_child.as_mut() {
            if child.try_wait().ok().flatten().is_some() {
                self.model_child = None;
                self.model_status = "模型进程已退出。".to_string();
            }
        }
    }

    fn current_video(&self) -> Option<&VideoMeta> { self.selected_index.and_then(|idx| self.videos.get(idx)) }
}

impl Drop for AiVideoApp {
    fn drop(&mut self) { self.stop_model(); }
}

fn load_texture_from_path(ctx: &egui::Context, path: &str, key: &str) -> Result<egui::TextureHandle, String> {
    let image = image::open(Path::new(path)).map_err(|err| err.to_string())?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Ok(ctx.load_texture(key.to_string(), color_image, egui::TextureOptions::LINEAR))
}

fn paint_texture_fit(ui: &egui::Ui, texture: &egui::TextureHandle, rect: egui::Rect) {
    ui.painter().rect_filled(rect, 4.0, egui::Color32::BLACK);
    let [tw, th] = texture.size();
    if tw == 0 || th == 0 { return; }
    let texture_aspect = tw as f32 / th as f32;
    let rect_aspect = rect.width() / rect.height();
    let draw_size = if texture_aspect > rect_aspect { egui::vec2(rect.width(), rect.width() / texture_aspect) } else { egui::vec2(rect.height() * texture_aspect, rect.height()) };
    let draw_rect = egui::Rect::from_center_size(rect.center(), draw_size);
    ui.painter().image(texture.id(), draw_rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
}

fn format_duration(seconds: f64) -> String {
    if seconds <= 0.0 { return "时长未知".to_string(); }
    let total = seconds.round() as u64;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins}:{secs:02}")
}

fn default_db_path() -> PathBuf { dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("ai-video").join("ai-video.sqlite3") }
fn nav_button(ui: &mut egui::Ui, label: &str) -> egui::Response { ui.add_sized([ui.available_width(), 36.0], egui::Button::new(label)) }
fn panel_frame() -> egui::Frame { egui::Frame::default().fill(egui::Color32::from_gray(24)).stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(48))).inner_margin(egui::Margin::same(8)) }
fn content_frame() -> egui::Frame { egui::Frame::default().fill(egui::Color32::from_gray(18)).inner_margin(egui::Margin::same(12)) }

fn apply_gray_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = egui::Color32::from_gray(22);
    visuals.panel_fill = egui::Color32::from_gray(24);
    visuals.extreme_bg_color = egui::Color32::from_gray(8);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(55);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(70);
    visuals.widgets.active.bg_fill = egui::Color32::from_gray(85);
    ctx.set_visuals(visuals);
}
