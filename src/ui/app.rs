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

pub struct AiVideoApp {
    mode: ScreenMode,
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
}

impl Default for AiVideoApp {
    fn default() -> Self {
        Self {
            mode: ScreenMode::Normal,
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
        }
    }
}

impl eframe::App for AiVideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("AI Video");
                ui.separator();
                ui.label(&self.scan_status);
            });
        });

        egui::SidePanel::left("left_sidebar")
            .resizable(true)
            .default_width(240.0)
            .show(ctx, |ui| self.left_sidebar(ui));

        if self.mode == ScreenMode::AiAnalysis {
            egui::SidePanel::right("right_video_list")
                .resizable(true)
                .default_width(220.0)
                .show(ctx, |ui| self.right_ai_video_list(ui));
        }

        egui::TopBottomPanel::bottom("bottom_chat")
            .resizable(true)
            .default_height(190.0)
            .show(ctx, |ui| self.bottom_chat(ui));

        egui::CentralPanel::default().show(ctx, |ui| match self.mode {
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

        if ui.button("打开文件夹").clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                self.folder = Some(folder.clone());
                self.scan_folder(folder);
            }
        }

        if ui.button("分析文件").clicked() {
            self.mode = ScreenMode::Normal;
            self.chat_log.push("系统：已切换到普通分析模式。".to_string());
        }

        if ui.button("AI 分析").clicked() {
            self.mode = ScreenMode::AiAnalysis;
            self.chat_log.push("系统：已切换到 AI 分析模式。".to_string());
        }

        if ui.button("设置").clicked() {
            self.mode = ScreenMode::Settings;
        }

        ui.separator();
        ui.label("搜索");
        let search_changed = ui.text_edit_singleline(&mut self.search_query).changed();
        if search_changed || ui.button("执行搜索").clicked() {
            self.search_current_database();
        }

        ui.separator();
        ui.label(format!("视频数量：{}", self.videos.len()));
        egui::ScrollArea::vertical().show(ui, |ui| {
            for idx in 0..self.videos.len() {
                let selected = self.selected_index == Some(idx);
                let label = self.videos[idx].name.clone();
                if ui.selectable_label(selected, label).clicked() {
                    self.selected_index = Some(idx);
                }
            }
        });
    }

    fn main_preview(&mut self, ui: &mut egui::Ui) {
        ui.heading("当前视频预览");
        ui.separator();
        if let Some(video) = self.current_video() {
            ui.label(format!("文件名：{}", video.name));
            ui.label(format!("路径：{}", video.path));
            ui.label(format!("时长：{:.2}s", video.duration));
            ui.label(format!("分辨率：{}x{}", video.width, video.height));
            ui.label(format!("FPS：{:.3}", video.fps));
            ui.label(format!("Hash：{}", video.hash));
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("左侧打开文件夹后，选择一个视频。后续将在这里接入播放器与缩略图预览。");
            });
        }
    }

    fn ai_analysis_view(&mut self, ui: &mut egui::Ui) {
        ui.heading("AI 分析工作区");
        ui.separator();
        let current = self.current_video().map(|video| (video.name.clone(), video.duration));
        if let Some((video_name, duration)) = current {
            ui.label(format!("当前视频：{}", video_name));
            ui.label("队列策略：单线程顺序分析；后台分析时禁止用户提问；暂停后允许针对当前视频提问。");
            ui.add_space(8.0);
            if ui.button("模拟开始分析当前视频").clicked() {
                self.chat_log.push(format!("系统：开始分析 {}", video_name));
                self.chat_log.push(format!("AI：已接收视频总时长 {:.2}s 与最大图片/音频限制。", duration));
            }
            if ui.button("暂停分析，允许提问").clicked() {
                self.chat_log.push("系统：分析已暂停，现在允许用户针对当前视频提问。".to_string());
            }
        } else {
            ui.label("请选择一个视频用于 AI 分析。");
        }
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.separator();
        ui.add(egui::Slider::new(&mut self.settings.max_images, 1..=64).text("最大图片数"));
        ui.add(egui::Slider::new(&mut self.settings.max_audio_segments, 0..=32).text("最大音频段数"));
        ui.add(egui::Slider::new(&mut self.settings.audio_clip_seconds, 1.0..=30.0).text("音频截取长度/s"));
        ui.add(egui::Slider::new(&mut self.settings.image_pixel_limit, 1000..=100000).text("图片压缩像素上限"));
        ui.add(egui::Slider::new(&mut self.settings.audio_sample_rate, 8000..=48000).text("音频采样率"));
        ui.add(egui::Slider::new(&mut self.settings.max_context_tokens, 1024..=65536).text("最大上下文 token"));
        ui.checkbox(&mut self.settings.debug_mode, "调试模式：显示原始 JSON");
    }

    fn right_ai_video_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("AI 队列");
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for idx in 0..self.videos.len() {
                let selected = self.selected_index == Some(idx);
                let text = format!("{}\n{:.1}s", self.videos[idx].name, self.videos[idx].duration);
                if ui.selectable_label(selected, text).clicked() {
                    self.selected_index = Some(idx);
                }
            }
        });
    }

    fn bottom_chat(&mut self, ui: &mut egui::Ui) {
        ui.heading("对话与调试输出");
        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
            for line in &self.chat_log {
                ui.label(line);
            }
        });
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.user_question);
            if ui.button("发送").clicked() {
                if !self.user_question.trim().is_empty() {
                    self.chat_log.push(format!("用户：{}", self.user_question.trim()));
                    self.chat_log.push("系统：当前为 UI 壳阶段，后续接入本地模型问答。".to_string());
                    self.user_question.clear();
                }
            }
        });
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
