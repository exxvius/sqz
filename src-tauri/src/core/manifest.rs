//! SQLite manifest: a durable, resumable, thread-safe record of every file.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::Serialize;

// Terminal statuses are never reprocessed on resume (unless the file changed).
pub const STATUS_PENDING: &str = "pending";
pub const STATUS_PROCESSING: &str = "processing";
pub const STATUS_DONE: &str = "done";
pub const STATUS_NORMALIZED: &str = "normalized";
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
    updated_at  REAL,
    forced      INTEGER DEFAULT 0,
    encode_ms   INTEGER
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
    /// Wall-clock encode time in milliseconds (only set on a real re-encode).
    pub encode_ms: Option<i64>,
}

/// A file row for the UI history view (any status).
#[derive(Debug, Clone, Serialize)]
pub struct HistoryRow {
    pub path: String,
    pub status: String,
    pub size: Option<u64>,
    pub src_codec: Option<String>,
    pub height: Option<u32>,
    pub out_size: Option<u64>,
    pub saved_bytes: Option<i64>,
    pub error: Option<String>,
    pub updated_at: Option<f64>,
}

/// Filters for a history query. Empty/`None` fields match everything.
#[derive(Debug, Default, Clone)]
pub struct HistoryQuery {
    /// Restrict to these statuses (empty = all).
    pub statuses: Vec<String>,
    /// Case-insensitive substring match on the path.
    pub search: Option<String>,
    pub limit: i64,
    pub offset: i64,
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
        // WAL + a busy timeout so the run thread and command connections (which
        // both write) don't trip over each other with SQLITE_BUSY.
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;",
        )?;
        conn.execute_batch(SCHEMA)?;
        // Migrations for manifests created before newer columns existed.
        let _ = conn.execute("ALTER TABLE files ADD COLUMN forced INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE files ADD COLUMN encode_ms INTEGER", []);
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
             height=COALESCE(?3, height), out_size=?4, saved_bytes=?5, error=?6, \
             encode_ms=COALESCE(?7, encode_ms), updated_at=?8 WHERE path=?9",
            params![
                status,
                upd.src_codec,
                upd.height,
                upd.out_size.map(|v| v as i64),
                upd.saved_bytes,
                upd.error,
                upd.encode_ms,
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

    /// Atomically claim the next pending file, marking it `processing` so no other
    /// worker takes it. `order` chooses which pending file wins. Returns the path
    /// and whether it was flagged force-process.
    pub fn claim_next_pending(
        &self,
        order: super::config::Order,
    ) -> rusqlite::Result<Option<ClaimedFile>> {
        use rusqlite::OptionalExtension;
        let conn = self.conn.lock().unwrap();
        // `order.sql()` returns a fixed internal fragment (never user input).
        let sql = format!(
            "SELECT path, COALESCE(forced, 0) FROM files WHERE status=?1 \
             ORDER BY {} LIMIT 1",
            order.sql()
        );
        let claimed: Option<(String, i64)> = conn
            .query_row(&sql, params![STATUS_PENDING], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if let Some((path, forced)) = &claimed {
            conn.execute(
                "UPDATE files SET status=?1, updated_at=?2 WHERE path=?3",
                params![STATUS_PROCESSING, now(), path],
            )?;
            return Ok(Some(ClaimedFile {
                path: path.clone(),
                forced: *forced != 0,
            }));
        }
        Ok(None)
    }

    /// Reset any rows left `processing` by a previous crash back to `pending`.
    pub fn recover_processing(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE files SET status=?1 WHERE status=?2",
            params![STATUS_PENDING, STATUS_PROCESSING],
        )?;
        Ok(())
    }

    /// Re-queue a file (retry). With `force`, it bypasses the skip checks.
    pub fn requeue(&self, path: &str, force: bool) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE files SET status=?1, forced=?2, error=NULL, out_size=NULL, \
             saved_bytes=NULL, updated_at=?3 WHERE path=?4",
            params![STATUS_PENDING, force as i64, now(), path],
        )?;
        Ok(())
    }

    pub fn total_reclaimed(&self) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(SUM(saved_bytes), 0) FROM files WHERE saved_bytes > 0",
            [],
            |r| r.get(0),
        )
    }

    /// Aggregate all-time statistics for the History dashboard.
    pub fn stats(&self) -> rusqlite::Result<Stats> {
        let conn = self.conn.lock().unwrap();
        let total_reclaimed: i64 = conn.query_row(
            "SELECT COALESCE(SUM(saved_bytes), 0) FROM files WHERE saved_bytes > 0",
            [],
            |r| r.get(0),
        )?;
        // Re-encode-only aggregates (status=done): time, throughput, ratio.
        let (encode_ms, files_encoded, bytes_in, bytes_out): (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT COALESCE(SUM(encode_ms), 0), COUNT(*), \
                 COALESCE(SUM(size), 0), COALESCE(SUM(out_size), 0) \
                 FROM files WHERE status=?1",
                params![STATUS_DONE],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        let files_touched: i64 =
            conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        Ok(Stats {
            total_reclaimed,
            encode_seconds: encode_ms as f64 / 1000.0,
            files_encoded,
            files_touched,
            bytes_in,
            bytes_out,
        })
    }

    /// Query history rows with optional status filter + path search, paged.
    pub fn history(&self, q: &HistoryQuery) -> rusqlite::Result<Vec<HistoryRow>> {
        use rusqlite::types::Value;
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT path, status, size, src_codec, height, out_size, saved_bytes, error, updated_at \
             FROM files",
        );
        let mut conds: Vec<String> = Vec::new();
        let mut args: Vec<Value> = Vec::new();

        if !q.statuses.is_empty() {
            let ph = vec!["?"; q.statuses.len()].join(",");
            conds.push(format!("status IN ({ph})"));
            args.extend(q.statuses.iter().map(|s| Value::Text(s.clone())));
        }
        if let Some(search) = &q.search {
            if !search.is_empty() {
                conds.push("LOWER(path) LIKE ?".into());
                args.push(Value::Text(format!("%{}%", search.to_lowercase())));
            }
        }
        if !conds.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conds.join(" AND "));
        }
        sql.push_str(" ORDER BY updated_at DESC LIMIT ? OFFSET ?");
        args.push(Value::Integer(if q.limit > 0 { q.limit } else { 500 }));
        args.push(Value::Integer(q.offset.max(0)));

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), row_to_history)?;
        rows.collect()
    }

    /// Delete one row from the manifest.
    pub fn delete_one(&self, path: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM files WHERE path=?1", params![path])?;
        Ok(())
    }

    /// Delete every row matching a history filter (used for "remove filtered").
    pub fn delete_matching(&self, q: &HistoryQuery) -> rusqlite::Result<usize> {
        use rusqlite::types::Value;
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from("DELETE FROM files");
        let mut conds: Vec<String> = Vec::new();
        let mut args: Vec<Value> = Vec::new();
        if !q.statuses.is_empty() {
            let ph = vec!["?"; q.statuses.len()].join(",");
            conds.push(format!("status IN ({ph})"));
            args.extend(q.statuses.iter().map(|s| Value::Text(s.clone())));
        }
        if let Some(search) = &q.search {
            if !search.is_empty() {
                conds.push("LOWER(path) LIKE ?".into());
                args.push(Value::Text(format!("%{}%", search.to_lowercase())));
            }
        }
        if !conds.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conds.join(" AND "));
        }
        conn.execute(&sql, rusqlite::params_from_iter(args))
    }

    /// Wipe the entire manifest.
    pub fn clear(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM files", [])?;
        Ok(())
    }
}

/// A file claimed for processing.
#[derive(Debug, Clone)]
pub struct ClaimedFile {
    pub path: String,
    pub forced: bool,
}

/// All-time aggregate statistics for the History dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    /// Total bytes reclaimed across every space-saving outcome.
    pub total_reclaimed: i64,
    /// Wall-clock seconds spent on real re-encodes (status=done).
    pub encode_seconds: f64,
    /// Number of files re-encoded.
    pub files_encoded: i64,
    /// Number of files ever recorded (any status).
    pub files_touched: i64,
    /// Sum of source bytes for re-encoded files.
    pub bytes_in: i64,
    /// Sum of output bytes for re-encoded files.
    pub bytes_out: i64,
}

fn row_to_history(r: &rusqlite::Row) -> rusqlite::Result<HistoryRow> {
    Ok(HistoryRow {
        path: r.get(0)?,
        status: r.get(1)?,
        size: r.get::<_, Option<i64>>(2)?.map(|v| v as u64),
        src_codec: r.get(3)?,
        height: r.get::<_, Option<i64>>(4)?.and_then(|v| u32::try_from(v).ok()),
        out_size: r.get::<_, Option<i64>>(5)?.map(|v| v as u64),
        saved_bytes: r.get(6)?,
        error: r.get(7)?,
        updated_at: r.get(8)?,
    })
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
    fn stats_aggregate_encode_time_and_ratio() {
        let m = mem_db();
        m.upsert_scanned("/a.mkv", 100, 1.0, false, true).unwrap();
        m.set_status(
            "/a.mkv",
            STATUS_DONE,
            &StatusUpdate {
                out_size: Some(40),
                saved_bytes: Some(60),
                encode_ms: Some(2_000),
                ..Default::default()
            },
        )
        .unwrap();
        m.upsert_scanned("/b.mkv", 200, 1.0, false, true).unwrap();
        m.set_status("/b.mkv", STATUS_SKIPPED_EFFICIENT, &StatusUpdate::default())
            .unwrap();

        let s = m.stats().unwrap();
        assert_eq!(s.total_reclaimed, 60);
        assert_eq!(s.files_encoded, 1); // only the done file
        assert_eq!(s.files_touched, 2);
        assert_eq!(s.bytes_in, 100);
        assert_eq!(s.bytes_out, 40);
        assert!((s.encode_seconds - 2.0).abs() < 1e-9);
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
