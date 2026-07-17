import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { fileName, humanBytes, relativeTime } from "../lib/format";
import { useStore } from "../lib/store";
import type { History } from "../lib/types";

export function HistoryView() {
  const store = useStore();
  const [history, setHistory] = useState<History | null>(null);

  // Refresh when opened and whenever a run finishes.
  useEffect(() => {
    api.getHistory().then(setHistory);
  }, [store.summary]);

  const done = history?.counts["done"] ?? 0;
  const failed = history?.counts["failed"] ?? 0;

  return (
    <div className="view">
      <div className="view-head">
        <h2>History</h2>
        <p>Every file sqz has re-encoded, and the space you've reclaimed over time.</p>
      </div>

      <div className="card">
        <div className="meter">
          <span className="num">{humanBytes(history?.total_saved ?? 0)}</span>
          <span className="muted">reclaimed all-time</span>
        </div>
        <div className="stat-row">
          <div className="stat">
            <span className="v">{done}</span>
            <span className="k">re-encoded</span>
          </div>
          <div className="stat">
            <span className="v">{failed}</span>
            <span className="k">failed</span>
          </div>
        </div>
      </div>

      <div className="card">
        <div className="card-title">Recently completed</div>
        {history && history.recent.length > 0 ? (
          history.recent.map((r) => (
            <div className="hist-row" key={r.path}>
              <span className="wname" title={r.path}>
                {fileName(r.path)}
              </span>
              <span className="saved">−{humanBytes(r.saved_bytes ?? 0)}</span>
              <span className="when">{relativeTime(r.updated_at)}</span>
            </div>
          ))
        ) : (
          <div className="empty">No completed files yet.</div>
        )}
      </div>
    </div>
  );
}
