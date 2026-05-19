//! Cache module facade. Heavy work lives in `core::cache_manager` so Tauri commands can expose a narrow API.

pub use crate::core::cache_manager::{
    clear_cache, default_cache_root, ensure_cache_layout, extract_audio_segment, extract_frames,
    extract_thumbnail, CachePaths,
};
