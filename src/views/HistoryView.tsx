import { useCallback, useEffect, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { StatusCard } from "../components/StatusCard";
import { FolderIcon, PlayIcon } from "../components/icons";
import { api, openFile, revealFile } from "../lib/api";
import { currentPath, fileName, humanBytes, relativeTime } from "../lib/format";
import { forceable, retryable, statusMeta } from "../lib/status";
import { useStore } from "../lib/store";
import type { History, Status } from "../lib/types";

const CHIPS: { id: Status; label: string }[] = [
  { id: "done", label: "Re-encoded" },
  { id: "normalized", label: "Normalized" },
  { id: "failed", label: "Failed" },
  { id: "skipped_no_gain", label: "No gain" },
  { id: "skipped_already_efficient", label: "Efficient" },
  { id: "skipped_marginal", label: "Lean" },
  { id: "pending", label: "Pending" },
  { id: "processing", label: "Processing" },
];

export function HistoryView() {
  const store = useStore();
  const [history, setHistory] = useState<History | null>(null);
  const [search, setSearch] = useState("");
  const [statuses, setStatuses] = useState<Set<Status>>(() => {
    try {
      const saved = localStorage.getItem("sqz-history-filter");
      if (saved) return new Set(JSON.parse(saved) as Status[]);
    } catch {
      /* ignore */
    }
    return new Set<Status>(["done"]);
  });

  useEffect(() => {
    localStorage.setItem("sqz-history-filter", JSON.stringify([...statuses]));
  }, [statuses]);

  const filter = useCallback(
    () => ({ search: search || undefined, statuses: [...statuses], limit: 1000 }),
    [search, statuses],
  );

  const refresh = useCallback(() => {
    api.getHistory(filter()).then(setHistory);
  }, [filter]);

  useEffect(refresh, [refresh, store.summary, store.running]);

  const toggle = (s: Status) =>
    setStatuses((prev) => {
      const next = new Set(prev);
      next.has(s) ? next.delete(s) : next.add(s);
      return next;
    });

  const removeFiltered = async () => {
    const count = history?.rows.length ?? 0;
    if (count === 0) return;
    const ok = await confirm(
      `Remove ${count} shown item${count === 1 ? "" : "s"} from the history database? This can't be undone.`,
      { title: "Remove from history", kind: "warning", okLabel: "Remove", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    await api.deleteHistoryMatching(filter());
    refresh();
  };
  const clearAll = async () => {
    const ok = await confirm(
      "Clear the entire history database? Every recorded file will be forgotten. This can't be undone.",
      { title: "Clear history", kind: "warning", okLabel: "Clear all", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    await api.clearHistory();
    refresh();
  };

  const act = async (fn: Promise<unknown>) => {
    await fn;
    refresh();
  };

  const counts = history?.counts ?? {};

  return (
    <div className="view">
      <div className="view-head">
        <h2>History</h2>
        <p>Every file sqz has read or touched. Search, filter, retry, or clean it up.</p>
      </div>

      <div className="card">
        <div className="meter">
          <span className="num">{humanBytes(history?.total_reclaimed ?? 0)}</span>
          <span className="muted">reclaimed all-time</span>
        </div>
      </div>

      <div className="card">
        <div className="filterbar">
          <input
            className="search"
            placeholder="Search by path…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <div className="chips-row">
            {CHIPS.map((c) => {
              const n = counts[c.id] ?? 0;
              return (
                <button
                  key={c.id}
                  className="chip"
                  aria-pressed={statuses.has(c.id)}
                  onClick={() => toggle(c.id)}
                >
                  {c.label}
                  {n > 0 && <span className="count-badge">{n}</span>}
                </button>
              );
            })}
          </div>
        </div>

        <div className="history-toolbar">
          <span className="muted">
            {history ? `${history.rows.length} shown` : "loading…"}
          </span>
          <div className="grow" />
          <button className="mini-btn danger" onClick={removeFiltered}>
            Remove shown
          </button>
          <button className="mini-btn danger" onClick={clearAll}>
            Clear all
          </button>
        </div>

        {history && history.rows.length > 0 ? (
          history.rows.map((r) => {
            const m = statusMeta(r.status);
            const savedTag =
              r.saved_bytes && r.saved_bytes > 0 ? (
                <span className="saved-tag">−{humanBytes(r.saved_bytes)}</span>
              ) : null;
            const encoded = r.status === "done" || r.status === "normalized";
            const filePath = currentPath(r.path, encoded);
            const actions = (
              <>
                {r.status !== "failed" && (
                  <>
                    <button className="mini-btn" onClick={() => openFile(filePath)}>
                      <PlayIcon /> Open
                    </button>
                    <button className="mini-btn" onClick={() => revealFile(filePath)}>
                      <FolderIcon /> Folder
                    </button>
                  </>
                )}
                {retryable(r.status) && (
                  <button className="mini-btn" onClick={() => act(api.retryFile(r.path))}>
                    ↻ Retry
                  </button>
                )}
                {forceable(r.status) && (
                  <button className="mini-btn" onClick={() => act(api.forceFile(r.path))}>
                    ⏵ Force process
                  </button>
                )}
                <button
                  className="mini-btn danger"
                  onClick={() => act(api.deleteHistoryItem(r.path))}
                >
                  ✕ Remove
                </button>
              </>
            );
            return (
              <StatusCard
                key={r.path}
                tone={m.tone}
                sym={m.sym}
                name={fileName(r.path)}
                fullPath={r.path}
                tag={m.label}
                meta={savedTag ?? <span className="ecard-meta">{relativeTime(r.updated_at)}</span>}
                actions={actions}
              >
                <dl className="kv-grid">
                  <dt>path</dt>
                  <dd>{r.path}</dd>
                  <dt>status</dt>
                  <dd>{m.label}</dd>
                  {r.src_codec && (
                    <>
                      <dt>source</dt>
                      <dd>
                        {r.src_codec}
                        {r.height ? ` · ${r.height}p` : ""}
                      </dd>
                    </>
                  )}
                  {r.size != null && (
                    <>
                      <dt>before</dt>
                      <dd>{humanBytes(r.size)}</dd>
                    </>
                  )}
                  {r.out_size != null && (
                    <>
                      <dt>after</dt>
                      <dd>{humanBytes(r.out_size)}</dd>
                    </>
                  )}
                  {r.saved_bytes != null && (
                    <>
                      <dt>saved</dt>
                      <dd>{humanBytes(r.saved_bytes)}</dd>
                    </>
                  )}
                  <dt>when</dt>
                  <dd>{relativeTime(r.updated_at)}</dd>
                </dl>

                {r.status === "failed" && r.error && <div className="err-box">{r.error}</div>}
              </StatusCard>
            );
          })
        ) : (
          <div className="empty">Nothing matches.</div>
        )}
      </div>
    </div>
  );
}
