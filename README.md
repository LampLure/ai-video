# ai-video

Rust + Tauri video tagging prototype. This scaffold follows the project plan:

- recursive video scanning and metadata hashing
- cache layout for thumbnails, frames and audio
- SQLite + FTS5 schema for video metadata and AI summaries
- llama.cpp-compatible AI client
- agent-style segment requests with limits
- single-threaded analysis queue control model
- grey UI shell with sidebar, preview area, right AI list and chat panel

## Development

```bash
cargo check
cargo run
```

The current implementation is a functional engineering scaffold. FFmpeg/mpv calls are isolated in the core/cache modules so they can be hardened without changing UI commands.
