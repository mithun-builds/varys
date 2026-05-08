use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Arc;

pub const KEY_OUTPUT_FOLDER: &str = "output_folder";
pub const KEY_MIC_GAIN: &str = "mic_gain";
pub const KEY_SYS_GAIN: &str = "sys_gain";
pub const KEY_WHISPER_MODEL: &str = "whisper_model";
pub const KEY_ONBOARDING_DISMISSED: &str = "onboarding_dismissed";
pub const KEY_MIC_PERMISSION_SEEN: &str = "mic_permission_seen";
pub const KEY_SCREEN_PERMISSION_SEEN: &str = "screen_permission_seen";

pub const DEFAULT_MIC_GAIN: f32 = 0.5;
pub const DEFAULT_SYS_GAIN: f32 = 0.5;

#[derive(Clone)]
pub struct Settings {
    conn: Arc<Mutex<Connection>>,
}

impl Settings {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)
            .with_context(|| format!("open settings db at {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS settings (
                 key        TEXT PRIMARY KEY,
                 value      TEXT NOT NULL,
                 updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );",
        )
        .context("init settings schema")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL,
                 updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        match stmt.query_row(params![key], |row| row.get::<_, String>(0)) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_or_default(&self, key: &str, default: &str) -> String {
        self.get(key).ok().flatten().unwrap_or_else(|| default.to_string())
    }

    pub fn get_f32(&self, key: &str, default: f32) -> f32 {
        self.get(key)
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    }

    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        match self.get(key).ok().flatten().as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        }
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Settings::in_memory().unwrap();
        assert!(s.get("x").unwrap().is_none());
        s.set("x", "hello").unwrap();
        assert_eq!(s.get("x").unwrap(), Some("hello".to_string()));
    }

    #[test]
    fn typed_getters() {
        let s = Settings::in_memory().unwrap();
        assert_eq!(s.get_f32("g", 0.7), 0.7);
        s.set("g", "0.42").unwrap();
        assert!((s.get_f32("g", 0.0) - 0.42).abs() < 1e-6);

        assert!(!s.get_bool("b", false));
        s.set("b", "true").unwrap();
        assert!(s.get_bool("b", false));
    }
}
