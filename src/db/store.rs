use crate::ai::schema::AnalysisResult;
use crate::core::video_manager::VideoMeta;
use crate::db::schema::INIT_SQL;
use anyhow::Result;
use rusqlite::{params, Connection};
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
        let json = serde_json::to_string(result)?;
        self.conn.execute(
            r#"INSERT INTO video_summaries(video_id, title, summary, structured_json, model_name)
               VALUES (?1, ?2, ?3, ?4, ?5)
               ON CONFLICT(video_id) DO UPDATE SET
                 title=excluded.title, summary=excluded.summary,
                 structured_json=excluded.structured_json, model_name=excluded.model_name,
                 generated_at=CURRENT_TIMESTAMP"#,
            params![video_id, result.title, result.summary, json, model_name],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if query.trim().is_empty() {
            return self.latest(limit);
        }
        let mut stmt = self.conn.prepare(
            r#"SELECT v.id, v.path, v.name, s.title, s.summary
               FROM video_search_fts f
               JOIN videos v ON v.id = f.video_id
               LEFT JOIN video_summaries s ON s.video_id = v.id
               WHERE video_search_fts MATCH ?1
               LIMIT ?2"#,
        )?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
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
