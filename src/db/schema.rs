pub const INIT_SQL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS videos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    hash TEXT NOT NULL UNIQUE,
    duration REAL NOT NULL DEFAULT 0,
    width INTEGER NOT NULL DEFAULT 0,
    height INTEGER NOT NULL DEFAULT 0,
    fps REAL NOT NULL DEFAULT 0,
    size INTEGER NOT NULL DEFAULT 0,
    mtime INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS video_summaries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    video_id INTEGER NOT NULL UNIQUE,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    structured_json TEXT NOT NULL,
    model_name TEXT NOT NULL,
    generated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(video_id) REFERENCES videos(id) ON DELETE CASCADE
);

CREATE VIRTUAL TABLE IF NOT EXISTS video_search_fts USING fts5(
    video_id UNINDEXED,
    title,
    summary,
    tags,
    scenes_text,
    content=''
);

CREATE TRIGGER IF NOT EXISTS video_summaries_ai AFTER INSERT ON video_summaries BEGIN
    INSERT INTO video_search_fts(video_id, title, summary, tags, scenes_text)
    VALUES (
        new.video_id,
        new.title,
        new.summary,
        json_extract(new.structured_json, '$.tags'),
        json_extract(new.structured_json, '$.scenes')
    );
END;

CREATE TRIGGER IF NOT EXISTS video_summaries_au AFTER UPDATE ON video_summaries BEGIN
    DELETE FROM video_search_fts WHERE video_id = old.video_id;
    INSERT INTO video_search_fts(video_id, title, summary, tags, scenes_text)
    VALUES (
        new.video_id,
        new.title,
        new.summary,
        json_extract(new.structured_json, '$.tags'),
        json_extract(new.structured_json, '$.scenes')
    );
END;

CREATE TRIGGER IF NOT EXISTS video_summaries_ad AFTER DELETE ON video_summaries BEGIN
    DELETE FROM video_search_fts WHERE video_id = old.video_id;
END;
"#;
