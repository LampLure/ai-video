use crate::core::settings::AppSettings;
use crate::core::video_manager::{scan_videos, VideoMeta};
use crate::db::{Database, SearchResult};
use eframe::egui;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScreenMode {
    Normal,
    AiAnalysis,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    AiLimits,
    Media,
    Cache,
    Debug,
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
}

impl Default for AiVideoApp {
    fn default() -> Self {
        Self {
            mode: ScreenMode::Normal,
            settings_section: SettingsSection::AiLimits,
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
        }
    }
}

impl eframe::App for AiVideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_gray_theme(ctx);

        egui::TopBottomPanel::top("top_bar")
            .frame(panel_frame())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("AI Video");
                    ui.separator();
                    ui.label(&self.scan_status);
                    if let Some(folder) = &self.folder {
                        ui.separator();
                        ui.small(format!("当前目录：{}", folder.display()));
                    }
                });
            });

        egui::SidePanel::left("left_sidebar")
            .resizable(true)
            .default_width(260.0)
            .width_range(180.0..=420.0)
            .frame(panel_frame())
            .show(ctx, |ui| self.left_sidebar(ui));

        if self.mode == ScreenMode::AiAnalysis {
            egui::SidePanel::right("right_video_list")
                .resizable(true)
                .default_width(230.0)
                .width_range(160.0..=360.0)
                .frame(panel_frame())
                .show(ctx, |ui| self.right_ai_video_list(ui));
        }

        egui::TopBottomPanel::bottom("bottom_chat")
            .resizable(true)
            .default_height(210.0)
            .height_range(120.0..=420.0)
            .frame(panel_frame())
            .show(ctx, |ui| self.bottom_chat(ui));

        egui::CentralPanel::default().frame(content_frame()).show(ctx, |ui| match self.mode {
            ScreenMode::Normal => self.main_preview(ui),
            ScreenMode::AiAnalysis => self.ai_analysis_view(ui),
            ScreenMode::Settings => self.settings_view(ui),
        });
    }
}

impl AiVideoApp {
    fn left_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| ui.heading("功能"));
        ui.add_space(8.0);

        if nav_button(ui, "打开文件夹").clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                self.folder = Some(folder.clone());
                self.scan_folder(folder);
            }
        }
        if nav_button(ui, "分析文件").clicked() {
            self.mode = ScreenMode::Normal;
            self.chat_log.push("系统：已切换到普通分析模式。".to_string());
        }
        if nav_button(ui, "AI 分析").clicked() {
            self.mode = ScreenMode::AiAnalysis;
            self.chat_log.push("系统：已切换到 AI 分析模式。".to_string());
        }
        if nav_button(ui, "设置").clicked() {
            self.mode = ScreenMode::Settings;
        }

        ui.separator();
        ui.label(egui::RichText::new("搜索").strong());
        ui.horizontal(|ui| {
            let search_changed = ui.text_edit_singleline(&mut self.search_query).changed();
            if search_changed || ui.button("搜索").clicked() {
                self.search_current_database();
            }
        });
        if !self.search_results.is_empty() {
            ui.collapsing(format!("搜索结果：{}", self.search_results.len()), |ui| {
                for result in &self.search_results {
                    ui.small(format!("{} - {}", result.video_id, result.title.as_deref().unwrap_or(&result.name)));
                }
            });
        }

        ui.separator();
        ui.label(egui::RichText::new(format!("视频列表：{}", self.videos.len())).strong());
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for idx in 0..self.videos.len() {
                self.video_card(ui, idx, false);
            }
        });
    }

    fn main_preview(&mut self, ui: &mut egui::Ui) {
        ui.heading("主体区域：当前视频预览 + 播放");
        ui.separator();
        self.preview_panel(ui, "普通模式：点击左侧视频后在这里播放");
    }

    fn ai_analysis_view(&mut self, ui: &mut egui::Ui) {
        ui.heading("AI 分析模式");
        ui.separator();
        self.preview_panel(ui, "AI 分析模式：点击右侧缩略图切换预览区");

        ui.add_space(10.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new("队列与控制").strong());
            ui.horizontal(|ui| {
                if ui.button("开始/恢复队列").clicked() {
                    self.ai_running = true;
                    self.ai_paused = false;
                    let current = self.current_video().map(|video| (video.name.clone(), video.duration));
                    if let Some((video_name, duration)) = current {
                        self.chat_log.push(format!("系统：开始分析 {}", video_name));
                        self.chat_log.push(format!(
                            "AI：已接收视频总时长 {:.2}s、最大图片数 {}、最大音频段数 {}。",
                            duration, self.settings.max_images, self.settings.max_audio_segments
                        ));
                    } else {
                        self.chat_log.push("系统：没有可分析的视频。".to_string());
                    }
                }
                if ui.button("暂停队列，允许提问").clicked() {
                    if self.ai_running {
                        self.ai_paused = true;
                        self.chat_log.push("系统：后台分析已暂停，现在允许用户针对当前视频提问。".to_string());
                    }
                }
            });
            let state = if self.ai_running && self.ai_paused {
                "已暂停，可提问"
            } else if self.ai_running {
                "后台分析中，禁止提问"
            } else {
                "未开始"
            };
            ui.label(format!("当前状态：{state}"));
        });
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.separator();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(180.0);
                ui.label(egui::RichText::new("设置导航").strong());
                self.settings_nav_button(ui, SettingsSection::AiLimits, "AI 限制");
                self.settings_nav_button(ui, SettingsSection::Media, "媒体处理");
                self.settings_nav_button(ui, SettingsSection::Cache, "缓存策略");
                self.settings_nav_button(ui, SettingsSection::Debug, "调试模式");
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("详细设置").strong());
                ui.add_space(8.0);
                match self.settings_section {
                    SettingsSection::AiLimits => {
                        ui.add(egui::Slider::new(&mut self.settings.max_images, 1..=64).text("最大图片数"));
                        ui.add(egui::Slider::new(&mut self.settings.max_audio_segments, 0..=32).text("最大音频段数"));
                        ui.add(egui::Slider::new(&mut self.settings.max_context_tokens, 1024..=65536).text("最大上下文 token 长度"));
                    }
                    SettingsSection::Media => {
                        ui.add(egui::Slider::new(&mut self.settings.audio_clip_seconds, 1.0..=30.0).text("音频截取长度/s"));
                        ui.add(egui::Slider::new(&mut self.settings.image_pixel_limit, 1000..=100000).text("图片压缩像素上限"));
                        ui.add(egui::Slider::new(&mut self.settings.audio_sample_rate, 8000..=48000).text("音频采样率"));
                    }
                    SettingsSection::Cache => {
                        ui.label("缓存目录结构：cache/thumbs、cache/frames、cache/audio");
                        ui.label("当前策略：扫描时写入数据库；后续接入按大小清理和切换文件夹清理策略。 ");
                    }
                    SettingsSection::Debug => {
                        ui.checkbox(&mut self.settings.debug_mode, "调试模式：显示原始 JSON");
                        ui.label("非调试模式下，聊天栏仅显示清洗后的文本。 ");
                    }
                }
            });
        });
    }

    fn right_ai_video_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("右侧栏：AI 队列");
        ui.separator();
        ui.label("竖直滚动视频缩略图 + 视频名");
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for idx in 0..self.videos.len() {
                self.video_card(ui, idx, true);
            }
        });
    }

    fn bottom_chat(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("下方栏：AI 对话");
            ui.separator();
            let prompt_state = if self.mode == ScreenMode::AiAnalysis && self.ai_running && !self.ai_paused {
                "后台分析中：输入禁用"
            } else {
                "可显示程序与 AI 消息"
            };
            ui.label(prompt_state);
        });
        egui::ScrollArea::vertical().max_height(130.0).auto_shrink([false, false]).show(ui, |ui| {
            for line in &self.chat_log {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(line);
                });
                ui.add_space(4.0);
            }
        });
        ui.horizontal(|ui| {
            let disabled = self.mode == ScreenMode::AiAnalysis && self.ai_running && !self.ai_paused;
            ui.add_enabled_ui(!disabled, |ui| {
                ui.text_edit_singleline(&mut self.user_question);
                if ui.button("发送").clicked() && !self.user_question.trim().is_empty() {
                    self.chat_log.push(format!("用户：{}", self.user_question.trim()));
                    self.chat_log.push("AI：当前为 UI 壳阶段，后续接入本地模型问答。".to_string());
                    self.user_question.clear();
                }
            });
        });
    }

    fn preview_panel(&mut self, ui: &mut egui::Ui, empty_hint: &str) {
        if let Some(video) = self.current_video() {
            let available_width = ui.available_width().max(320.0);
            let preview_height = (available_width * 9.0 / 16.0).clamp(220.0, 520.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(available_width, preview_height), egui::Sense::hover());
            ui.painter().rect_filled(rect, 8.0, egui::Color32::from_gray(28));
            ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, egui::Color32::from_gray(70)), egui::StrokeKind::Outside);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "视频播放预览区",
                egui::TextStyle::Heading.resolve(ui.style()),
                egui::Color32::from_gray(210),
            );
            ui.add_space(10.0);
            egui::Grid::new("video_meta_grid").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                ui.label("文件名"); ui.label(&video.name); ui.end_row();
                ui.label("路径"); ui.label(&video.path); ui.end_row();
                ui.label("时长"); ui.label(format!("{:.2}s", video.duration)); ui.end_row();
                ui.label("分辨率"); ui.label(format!("{}x{}", video.width, video.height)); ui.end_row();
                ui.label("FPS"); ui.label(format!("{:.3}", video.fps)); ui.end_row();
                ui.label("Hash"); ui.label(&video.hash); ui.end_row();
            });
        } else {
            ui.centered_and_justified(|ui| ui.label(empty_hint));
        }
    }

    fn video_card(&mut self, ui: &mut egui::Ui, idx: usize, compact: bool) {
        let selected = self.selected_index == Some(idx);
        let video = &self.videos[idx];
        let response = egui::Frame::group(ui.style())
            .fill(if selected { egui::Color32::from_gray(58) } else { egui::Color32::from_gray(32) })
            .show(ui, |ui| {
                let thumb_height = if compact { 70.0 } else { 82.0 };
                let width = ui.available_width().max(120.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, thumb_height), egui::Sense::click());
                ui.painter().rect_filled(rect, 4.0, egui::Color32::from_gray(18));
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "缩略图", egui::TextStyle::Small.resolve(ui.style()), egui::Color32::from_gray(160));
                ui.label(egui::RichText::new(&video.name).small());
                ui.small(format!("{:.1}s  {}x{}", video.duration, video.width, video.height));
            })
            .response;
        if response.clicked() {
            self.selected_index = Some(idx);
        }
        ui.add_space(6.0);
    }

    fn settings_nav_button(&mut self, ui: &mut egui::Ui, section: SettingsSection, label: &str) {
        if ui.selectable_label(self.settings_section == section, label).clicked() {
            self.settings_section = section;
        }
    }

    fn scan_folder(&mut self, folder: PathBuf) {
        self.scan_status = format!("正在扫描：{}", folder.display());
        match scan_videos(&folder.to_string_lossy()) {
            Ok(videos) => {
                self.videos = videos;
                self.selected_index = if self.videos.is_empty() { None } else { Some(0) };
                self.scan_status = format!("扫描完成：{} 个视频", self.videos.len());
                self.persist_scanned_videos();
            }
            Err(err) => {
                self.scan_status = format!("扫描失败：{err}");
            }
        }
    }

    fn persist_scanned_videos(&mut self) {
        match Database::open(&self.db_path) {
            Ok(db) => {
                for video in &self.videos {
                    let _ = db.upsert_video(video);
                }
            }
            Err(err) => self.chat_log.push(format!("数据库初始化失败：{err}")),
        }
    }

    fn search_current_database(&mut self) {
        match Database::open(&self.db_path).and_then(|db| db.search(&self.search_query, 50)) {
            Ok(results) => self.search_results = results,
            Err(err) => self.chat_log.push(format!("搜索失败：{err}")),
        }
    }

    fn current_video(&self) -> Option<&VideoMeta> {
        self.selected_index.and_then(|idx| self.videos.get(idx))
    }
}

fn default_db_path() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("ai-video").join("ai-video.sqlite3")
}

fn nav_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized([ui.available_width(), 30.0], egui::Button::new(label))
}

fn panel_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(egui::Color32::from_gray(24))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(48)))
        .inner_margin(egui::Margin::same(8))
}

fn content_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(egui::Color32::from_gray(18))
        .inner_margin(egui::Margin::same(10))
}

fn apply_gray_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = egui::Color32::from_gray(22);
    visuals.panel_fill = egui::Color32::from_gray(24);
    visuals.extreme_bg_color = egui::Color32::from_gray(8);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(45);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(65);
    visuals.widgets.active.bg_fill = egui::Color32::from_gray(80);
    ctx.set_visuals(visuals);
}
