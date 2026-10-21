use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub id: i64,
    pub source: String,
    pub target: String,
    pub src_lang: String,
    pub tgt_lang: String,
    pub model: String,
    pub created_at: i64,
    pub starred: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryRow {
    pub id: i64,
    pub term: String,
    pub translation: String,
    pub note: String,
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL,
            target TEXT NOT NULL,
            src_lang TEXT NOT NULL DEFAULT 'auto',
            tgt_lang TEXT NOT NULL DEFAULT 'zh',
            model TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            starred INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_hist_created ON history(created_at DESC);
         CREATE TABLE IF NOT EXISTS glossary (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            term TEXT NOT NULL,
            translation TEXT NOT NULL,
            note TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            UNIQUE(term, translation)
         );",
    )?;
    Ok(conn)
}

pub fn insert_history(
    conn: &Connection,
    source: &str,
    target: &str,
    src_lang: &str,
    tgt_lang: &str,
    model: &str,
) -> rusqlite::Result<i64> {
    let now = now_secs();
    conn.execute(
        "INSERT INTO history (source, target, src_lang, tgt_lang, model, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![source, target, src_lang, tgt_lang, model, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_history(conn: &Connection, query: &str, limit: i64) -> rusqlite::Result<Vec<HistoryRow>> {
    let like = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id, source, target, src_lang, tgt_lang, model, created_at, starred
         FROM history
         WHERE (?1 = '' OR source LIKE ?2 OR target LIKE ?2)
         ORDER BY created_at DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![query, like, limit], |r| {
        Ok(HistoryRow {
            id: r.get(0)?,
            source: r.get(1)?,
            target: r.get(2)?,
            src_lang: r.get(3)?,
            tgt_lang: r.get(4)?,
            model: r.get(5)?,
            created_at: r.get(6)?,
            starred: r.get::<_, i64>(7)? != 0,
        })
    })?;
    rows.collect()
}

pub fn delete_history(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn toggle_star(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    conn.execute(
        "UPDATE history SET starred = 1 - starred WHERE id = ?1",
        params![id],
    )?;
    let v: i64 = conn.query_row("SELECT starred FROM history WHERE id = ?1", params![id], |r| {
        r.get(0)
    })?;
    Ok(v != 0)
}

pub fn clear_history(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM history WHERE starred = 0", [])?;
    Ok(())
}

pub fn list_glossary(conn: &Connection) -> rusqlite::Result<Vec<GlossaryRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, term, translation, note FROM glossary ORDER BY term COLLATE NOCASE ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(GlossaryRow {
            id: r.get(0)?,
            term: r.get(1)?,
            translation: r.get(2)?,
            note: r.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn add_glossary(
    conn: &Connection,
    term: &str,
    translation: &str,
    note: &str,
) -> rusqlite::Result<i64> {
    let now = now_secs();
    conn.execute(
        "INSERT OR REPLACE INTO glossary (term, translation, note, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![term, translation, note, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_glossary(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM glossary WHERE id = ?1", params![id])?;
    Ok(())
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
