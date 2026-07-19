import { useCallback, useEffect, useState } from "react";
import { message, save } from "@tauri-apps/plugin-dialog";
import { StatusCard } from "../components/StatusCard";
import { useConfirm } from "../components/ConfirmModal";
import { FolderIcon, PlayIcon } from "../components/icons";
import { api, openFile, revealFile } from "../lib/api";
import {
  currentPath,
  fileName,
  fmtDuration,
  fmtDurationLong,
  fmtRate,
  humanBytes,
  pct,
  relativeTime,
} from "../lib/format";
import { forceable, retryable, statusMeta } from "../lib/status";
import { useStore } from "../lib/store";
import { useLock } from "../lib/lock";
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

const PAGE_SIZE = 25;

export function HistoryView() {
  const store = useStore();
  const { locked, maskName, maskPath } = useLock();
  const { confirm, element: confirmModal } = useConfirm();
  const [history, setHistory] = useState<History | null>(null);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
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

  // Any filter change resets to the first page.
  useEffect(() => setPage(0), [search, statuses]);

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
    const ok = await confirm({
      title: "Remove from history",
      message: `Remove ${count} shown item${count === 1 ? "" : "s"} from the history database? This can't be undone.`,
      confirmLabel: "Remove",
      danger: true,
    });
    if (!ok) return;
    await api.deleteHistoryMatching(filter());
    refresh();
  };
  const clearAll = async () => {
    const ok = await confirm({
      title: "Clear history",
      message:
        "Clear the entire history database? Every recorded file will be forgotten. This can't be undone.",
      confirmLabel: "Clear all",
      danger: true,
    });
    if (!ok) return;
    await api.clearHistory();
    refresh();
  };

  const act = async (fn: Promise<unknown>) => {
    await fn;
    refresh();
  };

  const restore = async (path: string) => {
    const ok = await confirm({
      title: "Restore original",
      message:
        "Restore the original and send the encoded file to the Recycle Bin? This only works when the original was kept in a holding folder.",
      confirmLabel: "Restore",
    });
    if (!ok) return;
    try {
      await api.restoreOriginal(path);
      refresh();
    } catch (e) {
      await message(e instanceof Error ? e.message : "Restore failed.", {
        title: "Restore failed",
        kind: "error",
      });
    }
  };

  const exportHistory = async () => {
    const dest = await save({
      defaultPath: "sqz-history.csv",
      filters: [
        { name: "CSV", extensions: ["csv"] },
        { name: "JSON", extensions: ["json"] },
      ],
    });
    if (!dest) return;
    const format = dest.toLowerCase().endsWith(".json") ? "json" : "csv";
    const n = await api.exportHistory(dest, format, filter());
    await message(`Exported ${n} row${n === 1 ? "" : "s"}.`, { title: "Export complete" });
  };

  const counts = history?.counts ?? {};

  // Client-side pagination: only PAGE_SIZE cards are rendered at once — rendering
  // thousands of collapsible rows is what bogs the page down.
  const allRows = history?.rows ?? [];
  const totalPages = Math.max(1, Math.ceil(allRows.length / PAGE_SIZE));
  const clampedPage = Math.min(page, totalPages - 1);
  const pageStart = clampedPage * PAGE_SIZE;
  const pageRows = allRows.slice(pageStart, pageStart + PAGE_SIZE);

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
        {history && (
          <div className="stat-row">
            <div className="stat">
              <span className="v">{fmtDurationLong(history.encode_seconds)}</span>
              <span className="k">Time encoding</span>
            </div>
            <div className="stat">
              <span className="v">{fmtRate(history.total_reclaimed, history.encode_seconds)}</span>
              <span className="k">Efficiency</span>
            </div>
            <div className="stat">
              <span className="v">
                {history.bytes_in > 0
                  ? pct(1 - history.bytes_out / history.bytes_in)
                  : "—"}
              </span>
              <span className="k">Avg reduction</span>
            </div>
            <div className="stat">
              <span className="v">{history.files_encoded.toLocaleString()}</span>
              <span className="k">Re-encoded</span>
            </div>
            <div className="stat">
              <span className="v">
                {history.files_encoded > 0
                  ? fmtDuration(history.encode_seconds / history.files_encoded)
                  : "—"}
              </span>
              <span className="k">Avg / file</span>
            </div>
            <div className="stat">
              <span className="v">{history.files_touched.toLocaleString()}</span>
              <span className="k">Files tracked</span>
            </div>
          </div>
        )}
      </div>

      <div className="card card-flat">
        <div className="filterbar">
          <input
            className="search"
            placeholder={locked ? "Search disabled while locked" : "Search by path…"}
            value={locked ? "" : search}
            disabled={locked}
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
            {history
              ? allRows.length === 0
                ? "0 shown"
                : `${pageStart + 1}–${pageStart + pageRows.length} of ${allRows.length}`
              : "loading…"}
          </span>
          <div className="grow" />
          {locked ? (
            <span className="muted" style={{ fontSize: "var(--text-xs)" }}>
              Editing &amp; export locked
            </span>
          ) : (
            <>
              <button className="mini-btn" onClick={exportHistory}>
                ⭳ Export
              </button>
              <button className="mini-btn danger" onClick={removeFiltered}>
                Remove shown
              </button>
              <button className="mini-btn danger" onClick={clearAll}>
                Clear all
              </button>
            </>
          )}
        </div>

        {history && allRows.length > 0 ? (
          pageRows.map((r) => {
            const m = statusMeta(r.status);
            const savedTag =
              r.saved_bytes && r.saved_bytes > 0 ? (
                <span className="saved-tag">−{humanBytes(r.saved_bytes)}</span>
              ) : null;
            const encoded = r.status === "done" || r.status === "normalized";
            const filePath = currentPath(r.path, encoded);
            const actions = locked ? null : (
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
                {encoded && (
                  <button className="mini-btn" onClick={() => restore(r.path)}>
                    ↶ Restore original
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
                name={maskName(fileName(r.path))}
                fullPath={locked ? undefined : r.path}
                tag={m.label}
                meta={savedTag ?? <span className="ecard-meta">{relativeTime(r.updated_at)}</span>}
                actions={actions}
              >
                <dl className="kv-grid">
                  <dt>path</dt>
                  <dd>{maskPath(r.path)}</dd>
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

        {totalPages > 1 && (
          <div className="pager">
            <button
              className="mini-btn"
              disabled={clampedPage === 0}
              onClick={() => setPage(clampedPage - 1)}
            >
              ‹ Prev
            </button>
            <span className="muted">
              Page {clampedPage + 1} of {totalPages}
            </span>
            <button
              className="mini-btn"
              disabled={clampedPage >= totalPages - 1}
              onClick={() => setPage(clampedPage + 1)}
            >
              Next ›
            </button>
          </div>
        )}
      </div>
      {confirmModal}
    </div>
  );
}
