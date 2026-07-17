//! SQLite manifest: a durable, resumable, thread-safe record of every file.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::Serialize;

// Terminal statuses are never reprocessed on resume (unless the file changed).
pub const STATUS_PENDING: &str = "pending";
pub const STATUS_DONE: &str = "done";
pub const STATUS_SKIPPED_EFFICIENT: &str = "skipped_already_efficient";
pub const STATUS_SKIPPED_NO_GAIN: &str = "skipped_no_gain";
pub const STATUS_SKIPPED_MARGINAL: &str = "skipped_marginal";
pub const STATUS_FAILED: &str = "failed";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS files (
    path        TEXT PRIMARY KEY,
    size        INTEGER,
    mtime       REAL,
    status      TEXT NOT NULL,
    src_codec   TEXT,
    height      INTEGER,
    out_size    INTEGER,
    saved_bytes INTEGER,
    error       TEXT,
    updated_at  REAL
);
CREATE INDEX IF NOT EXISTS idx_files_status ON files(status);
";

/// Fields a `set_status` call may update. `None` leaves prior values intact for
/// the COALESCE-guarded columns (src_codec, height).
#[derive(Debug, Default, Clone)]
pub struct StatusUpdate {
    pub src_codec: Option<String>,
    pub height: Option<u32>,
    pub out_size: Option<u64>,
    pub saved_bytes: Option<i64>,
    pub error: Option<String>,
}

/// A completed-file row for the UI history view.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryRow {
    pub path: String,
    pub src_codec: Option<String>,
    pub height: Option<u32>,
    pub out_size: Option<u64>,
    pub saved_bytes: Option<i64>,
    pub updated_at: Option<f64>,
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// One shared connection guarded by a mutex (writes are short and serialized).
pub struct Manifest {
    conn: Mutex<Connection>,
}

impl Manifest {
    pub fn open(db_path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(db_path)?;
        // journal_mode=WAL returns a result row, so it must go through
        // execute_batch (pragma_update rejects statements that yield rows).
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Register a discovered file. New → pending. Reset to pending when the file
    /// changed (size/mtime differ), when `force` is set, or when it previously
    /// failed and `retry_failed` is on.
    pub fn upsert_scanned(
        &self,
        path: &str,
        size: u64,
        mtime: f64,
        force: bool,
        retry_failed: bool,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let existing: Option<(i64, f64, String)> = conn
            .query_row(
                "SELECT size, mtime, status FROM files WHERE path=?1",
                params![path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();

        match existing {
            None => {
                conn.execute(
                    "INSERT INTO files(path, size, mtime, status, updated_at) VALUES(?1,?2,?3,?4,?5)",
                    params![path, size as i64, mtime, STATUS_PENDING, now()],
                )?;
            }
            Some((old_size, old_mtime, status)) => {
                let changed = old_size != size as i64 || (old_mtime - mtime).abs() > f64::EPSILON;
                let failed_retry = retry_failed && status == STATUS_FAILED;
                if force || changed || failed_retry {
                    conn.execute(
                        "UPDATE files SET size=?1, mtime=?2, status=?3, error=NULL, \
                         out_size=NULL, saved_bytes=NULL, updated_at=?4 WHERE path=?5",
                        params![size as i64, mtime, STATUS_PENDING, now(), path],
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn set_status(&self, path: &str, status: &str, upd: &StatusUpdate) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE files SET status=?1, src_codec=COALESCE(?2, src_codec), \
             height=COALESCE(?3, height), out_size=?4, saved_bytes=?5, error=?6, updated_at=?7 \
             WHERE path=?8",
            params![
                status,
                upd.src_codec,
                upd.height,
                upd.out_size.map(|v| v as i64),
                upd.saved_bytes,
                upd.error,
                now(),
                path,
            ],
        )?;
        Ok(())
    }

    pub fn pending_paths(&self) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT path FROM files WHERE status=?1 ORDER BY path")?;
        let rows = stmt.query_map(params![STATUS_PENDING], |r| r.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn status_counts(&self) -> rusqlite::Result<HashMap<String, i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM files GROUP BY status")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut map = HashMap::new();
        for row in rows {
            let (k, v) = row?;
            map.insert(k, v);
        }
        Ok(map)
    }

    pub fn total_saved_bytes(&self) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(SUM(saved_bytes), 0) FROM files WHERE status=?1",
            params![STATUS_DONE],
            |r| r.get(0),
        )
    }

    /// Most-recently completed files, newest first, for the history view.
    pub fn recent_done(&self, limit: i64) -> rusqlite::Result<Vec<HistoryRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT path, src_codec, height, out_size, saved_bytes, updated_at \
             FROM files WHERE status=?1 ORDER BY updated_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![STATUS_DONE, limit], |r| {
            Ok(HistoryRow {
                path: r.get(0)?,
                src_codec: r.get(1)?,
                height: r.get::<_, Option<i64>>(2)?.and_then(|v| u32::try_from(v).ok()),
                out_size: r.get::<_, Option<i64>>(3)?.map(|v| v as u64),
                saved_bytes: r.get(4)?,
                updated_at: r.get(5)?,
            })
        })?;
        rows.collect()
    }
}

/// Current mtime of a path as fractional Unix seconds (for change detection).
pub fn mtime_secs(meta: &std::fs::Metadata) -> f64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Manifest {
        let p = std::env::temp_dir().join(format!("sqz_m_{}.db", uuid::Uuid::new_v4()));
        Manifest::open(&p).unwrap()
    }

    #[test]
    fn new_file_is_pending_then_terminal() {
        let m = mem_db();
        m.upsert_scanned("/a.mkv", 100, 1.0, false, true).unwrap();
        assert_eq!(m.pending_paths().unwrap(), vec!["/a.mkv".to_string()]);

        m.set_status(
            "/a.mkv",
            STATUS_DONE,
            &StatusUpdate {
                out_size: Some(40),
                saved_bytes: Some(60),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(m.pending_paths().unwrap().is_empty());
        assert_eq!(m.total_saved_bytes().unwrap(), 60);
    }

    #[test]
    fn unchanged_terminal_file_is_not_reprocessed() {
        let m = mem_db();
        m.upsert_scanned("/a.mkv", 100, 1.0, false, true).unwrap();
        m.set_status("/a.mkv", STATUS_DONE, &StatusUpdate::default()).unwrap();
        // Same size/mtime, no force → stays done.
        m.upsert_scanned("/a.mkv", 100, 1.0, false, true).unwrap();
        assert!(m.pending_paths().unwrap().is_empty());
    }

    #[test]
    fn changed_file_is_requeued() {
        let m = mem_db();
        m.upsert_scanned("/a.mkv", 100, 1.0, false, true).unwrap();
        m.set_status("/a.mkv", STATUS_DONE, &StatusUpdate::default()).unwrap();
        m.upsert_scanned("/a.mkv", 200, 2.0, false, true).unwrap(); // changed
        assert_eq!(m.pending_paths().unwrap(), vec!["/a.mkv".to_string()]);
    }

    #[test]
    fn failed_is_retried_by_default_but_not_when_disabled() {
        let m = mem_db();
        m.upsert_scanned("/a.mkv", 100, 1.0, false, true).unwrap();
        m.set_status("/a.mkv", STATUS_FAILED, &StatusUpdate::default()).unwrap();

        m.upsert_scanned("/a.mkv", 100, 1.0, false, false).unwrap(); // retry off
        assert!(m.pending_paths().unwrap().is_empty());

        m.upsert_scanned("/a.mkv", 100, 1.0, false, true).unwrap(); // retry on
        assert_eq!(m.pending_paths().unwrap(), vec!["/a.mkv".to_string()]);
    }
}
