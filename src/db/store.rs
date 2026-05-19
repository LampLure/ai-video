use crate::ai::schema::{AnalysisResult, SceneSummary};
use crate::core::video_manager::VideoMeta;
use crate::db::schema::INIT_SQL;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub video_id: i64,
    pub path: String,
    pub name: String,
    pub title: Option<String>,
    pub summary: Option<String>,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(INIT_SQL)?;
        Ok(Self { conn })
    }

    pub fn upsert_video(&self, video: &VideoMeta) -> Result<i64> {
        self.conn.execute(
            r#"INSERT INTO videos(path, name, hash, duration, width, height, fps, size, mtime)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
               ON CONFLICT(path) DO UPDATE SET
                 name=excluded.name, hash=excluded.hash, duration=excluded.duration,
                 width=excluded.width, height=excluded.height, fps=excluded.fps,
                 size=excluded.size, mtime=excluded.mtime, updated_at=CURRENT_TIMESTAMP"#,
            params![video.path, video.name, video.hash, video.duration, video.width, video.height, video.fps, video.size as i64, video.mtime],
        )?;
        let id = self.conn.query_row("SELECT id FROM videos WHERE path = ?1", params![video.path], |row| row.get(0))?;
        Ok(id)
    }

    pub fn save_summary(&self, video_id: i64, result: &AnalysisResult, model_name: &str) -> Result<()> {
        let structured_json = serde_json::to_string(result)?;
        let tags_text = result.tags.join(" ");
        let scenes_text = scenes_to_text(&result.scenes);
        self.conn.execute(
            r#"INSERT INTO video_summaries(video_id, title, summary, tags_text, scenes_text, structured_json, model_name)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
               ON CONFLICT(video_id) DO UPDATE SET
                 title=excluded.title, summary=excluded.summary,
                 tags_text=excluded.tags_text, scenes_text=excluded.scenes_text,
                 structured_json=excluded.structured_json, model_name=excluded.model_name,
                 generated_at=CURRENT_TIMESTAMP"#,
            params![video_id, result.title, result.summary, tags_text, scenes_text, structured_json, model_name],
        )?;
        self.refresh_fts_row(video_id, &result.title, &result.summary, &tags_text, &scenes_text)?;
        Ok(())
    }

    pub fn get_summary_by_hash(&self, video_hash: &str) -> Result<Option<AnalysisResult>> {
        let json: Option<String> = self.conn.query_row(
            r#"SELECT s.structured_json
               FROM video_summaries s
               JOIN videos v ON v.id = s.video_id
               WHERE v.hash = ?1"#,
            params![video_hash],
            |row| row.get(0),
        ).optional()?;
        if let Some(value) = json { Ok(Some(serde_json::from_str(&value)?)) } else { Ok(None) }
    }

    fn refresh_fts_row(&self, video_id: i64, title: &str, summary: &str, tags: &str, scenes_text: &str) -> Result<()> {
        self.conn.execute("DELETE FROM video_search_fts WHERE video_id = ?1", params![video_id])?;
        self.conn.execute(
            "INSERT INTO video_search_fts(video_id, title, summary, tags, scenes_text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![video_id, title, summary, tags, scenes_text],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.latest(limit);
        }
        let fts_query = make_fts_query(trimmed);
        let mut stmt = self.conn.prepare(
            r#"SELECT v.id, v.path, v.name, s.title, s.summary
               FROM video_search_fts f
               JOIN videos v ON v.id = f.video_id
               LEFT JOIN video_summaries s ON s.video_id = v.id
               WHERE video_search_fts MATCH ?1
               ORDER BY rank
               LIMIT ?2"#,
        )?;
        let rows = stmt.query_map(params![fts_query, limit as i64], |row| {
            Ok(SearchResult {
                video_id: row.get(0)?, path: row.get(1)?, name: row.get(2)?, title: row.get(3)?, summary: row.get(4)?,
            })
        })?;
        Ok(rows.filter_map(|row| row.ok()).collect())
    }

    pub fn latest(&self, limit: usize) -> Result<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT v.id, v.path, v.name, s.title, s.summary
               FROM videos v
               LEFT JOIN video_summaries s ON s.video_id = v.id
               ORDER BY v.updated_at DESC
               LIMIT ?1"#,
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SearchResult { video_id: row.get(0)?, path: row.get(1)?, name: row.get(2)?, title: row.get(3)?, summary: row.get(4)? })
        })?;
        Ok(rows.filter_map(|row| row.ok()).collect())
    }
}

fn scenes_to_text(scenes: &[SceneSummary]) -> String {
    scenes
        .iter()
        .map(|scene| format!("{:.3}-{:.3} {} {}", scene.start, scene.end, scene.description, scene.tags.join(" ")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn make_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|token| !token.trim().is_empty())
        .map(|token| format!("\"{}\"", token.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" OR ")
}
