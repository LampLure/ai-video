use ai_video::ui::commands::*;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_videos,
            get_default_settings,
            save_settings,
            init_database,
            search_videos,
            start_analysis_queue,
            pause_analysis_queue,
            ask_current_video,
            prepare_video_segment
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ai-video application");
}
