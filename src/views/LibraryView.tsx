import { useCallback, useEffect, useRef, useState } from "react";
import { StatusCard } from "../components/StatusCard";
import { useConfirm } from "../components/ConfirmModal";
import { LibraryEditor } from "../components/LibraryEditor";
import {
  DeepScanIcon,
  FolderIcon,
  LibraryIcon,
  PlayIcon,
  SearchIcon,
} from "../components/icons";
import { api, openFile, pickInputs, revealFile } from "../lib/api";
import { defaultConfig } from "../lib/config";
import { fileName, humanBytes, pct, relativeTime } from "../lib/format";
import { healthMeta, statusMeta } from "../lib/status";
import { useLock } from "../lib/lock";
import { useStore } from "../lib/store";
import type { HealthState, Library, RunConfig, SavedLibrary } from "../lib/types";

const CODEC_LABEL: Record<string, string> = { av1: "AV1", hevc: "HEVC", h264: "H.264" };
const QUALITY_LABEL: Record<string, string> = {
  "max-savings": "Max savings",
  balanced: "Balanced",
  "high-quality": "High quality",
  "visually-lossless": "Visually lossless",
};

/** One-line "AV1 · Balanced · ≤1080p · MKV" summary of a library's profile. */
function profileSummary(p: RunConfig): string {
  const parts = [CODEC_LABEL[p.codec] ?? p.codec];
  parts.push(p.vmaf_target != null ? `VMAF ${p.vmaf_target}` : QUALITY_LABEL[p.quality] ?? p.quality);
  parts.push(p.max_height > 4320 ? "no cap" : `≤${p.max_height}p`);
  parts.push(p.container.toUpperCase());
  return parts.join(" · ");
}

const HEALTH_CHIPS: { id: HealthState; label: string }[] = [
  { id: "healthy", label: "Healthy" },
  { id: "corrupt", label: "Corrupt" },
  { id: "unreadable", label: "Unreadable" },
];

const PAGE_SIZE = 25;
const ROOTS_KEY = "sqz-library-roots";

function loadRoots(): string[] {
  try {
    const saved = localStorage.getItem(ROOTS_KEY);
    if (saved) return JSON.parse(saved) as string[];
  } catch {
    /* ignore */
  }
  return [];
}

export function LibraryView({ config }: { config: RunConfig }) {
  const { locked, maskName, maskPath } = useLock();
  const { confirm, element: confirmModal } = useConfirm();
  // Scan state lives in the shared store so its progress survives leaving and
  // returning to this tab (the scan runs in the background regardless).
  const store = useStore();
  const { scanning, scanDeep, scanProgress, scanError } = store;
  const [library, setLibrary] = useState<Library | null>(null);
  const [roots, setRoots] = useState<string[]>(loadRoots);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [filters, setFilters] = useState<Set<HealthState>>(new Set());
  const [libraries, setLibraries] = useState<SavedLibrary[]>([]);
  const [editing, setEditing] = useState<SavedLibrary | null>(null);

  const refreshLibraries = useCallback(() => {
    api.listLibraries().then(setLibraries);
  }, []);

  useEffect(refreshLibraries, [refreshLibraries]);

  useEffect(() => {
    localStorage.setItem(ROOTS_KEY, JSON.stringify(roots));
  }, [roots]);

  useEffect(() => setPage(0), [search, filters]);

  const refresh = useCallback(() => {
    api.getLibrary({ limit: 5000 }).then(setLibrary);
  }, []);

  useEffect(refresh, [refresh]);

  // A scan just finished (scanning fell to false) — its results are now in the
  // DB, so pull the fresh library. Also covers a scan that completed while this
  // tab was unmounted, then the user returned.
  const wasScanning = useRef(scanning);
  useEffect(() => {
    if (wasScanning.current && !scanning) refresh();
    wasScanning.current = scanning;
  }, [scanning, refresh]);

  const addFolders = async () => {
    const picked = await pickInputs(true);
    if (picked.length === 0) return;
    setRoots((prev) => [...new Set([...prev, ...picked])]);
  };

  const removeRoot = (root: string) => setRoots((prev) => prev.filter((r) => r !== root));

  const runScan = (deep: boolean) => {
    if (roots.length === 0 || scanning) return;
    store.startScan({ ...config, inputs: roots }, deep);
  };

  // A library encodes to its own embedded profile: profile + roots → the
  // existing run/scan paths.
  const runLibrary = (lib: SavedLibrary) => {
    if (store.running) return;
    store.start({ ...lib.profile, inputs: lib.roots });
  };
  const scanLibrary = (lib: SavedLibrary, deep: boolean) => {
    if (scanning) return;
    store.startScan({ ...lib.profile, inputs: lib.roots }, deep);
  };

  const newLibrary = (): SavedLibrary => ({
    id: "",
    name: "",
    roots: [],
    profile: { ...defaultConfig(), inputs: [] },
    created_at: 0,
    updated_at: 0,
  });

  // "Save as library" captures the current ad-hoc folders + active config.
  const saveCurrentAsLibrary = () =>
    setEditing({ ...newLibrary(), roots, profile: { ...config, inputs: [] } });

  const saveLibrary = async (lib: SavedLibrary) => {
    await api.saveLibrary(lib);
    refreshLibraries();
  };

  const removeLibrary = async (lib: SavedLibrary) => {
    const ok = await confirm({
      title: "Delete library",
      message: `Delete the "${lib.name}" library? This removes the saved folders and profile only — your files and their history are untouched.`,
      confirmLabel: "Delete",
      danger: true,
    });
    if (!ok) return;
    await api.deleteLibrary(lib.id);
    refreshLibraries();
  };

  const toggle = (id: HealthState) =>
    setFilters((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });

  const scanFrac =
    scanProgress && scanProgress.total > 0 ? scanProgress.scanned / scanProgress.total : 0;

  const counts = library?.counts ?? {};
  const allRows = (library?.rows ?? []).filter((r) => {
    if (filters.size > 0 && (r.health === null || !filters.has(r.health))) return false;
    if (search && !r.path.toLowerCase().includes(search.toLowerCase())) return false;
    return true;
  });
  const totalPages = Math.max(1, Math.ceil(allRows.length / PAGE_SIZE));
  const clampedPage = Math.min(page, totalPages - 1);
  const pageStart = clampedPage * PAGE_SIZE;
  const pageRows = allRows.slice(pageStart, pageStart + PAGE_SIZE);

  const flagged = (counts["corrupt"] ?? 0) + (counts["unreadable"] ?? 0);

  // Every Library row is a scanned file, so all are removable. Removing just
  // clears the health record (and deletes scan-only rows) — it never touches
  // encode history, so predictions and the History view are unaffected.
  const removeOne = async (path: string) => {
    await api.deleteLibraryPaths([path]);
    refresh();
  };

  const removeShown = async () => {
    if (allRows.length === 0) return;
    const n = allRows.length;
    const ok = await confirm({
      title: "Remove from library",
      message: `Remove ${n} file${
        n === 1 ? "" : "s"
      } from the library health list? This clears their scan result only — encode history is kept, and the files on disk are untouched.`,
      confirmLabel: "Remove",
      danger: true,
    });
    if (!ok) return;
    await api.deleteLibraryPaths(allRows.map((r) => r.path));
    refresh();
  };

  return (
    <div className="view">
      <div className="view-head">
        <h2>Library</h2>
        <p>
          Save your media folders as named libraries, each with its own encode profile (movies vs.
          phone clips vs. VR), and re-run or health-check them in one click. Scanning flags corrupt
          or unreadable files without re-encoding; scanned files show up below.
        </p>
      </div>

      <div className="card">
        <div className="meter">
          <span className="num">{(library?.rows.length ?? 0).toLocaleString()}</span>
          <span className="muted">files scanned</span>
        </div>
        <div className="stat-row">
          <div className="stat">
            <span className="v">{(counts["healthy"] ?? 0).toLocaleString()}</span>
            <span className="k">Healthy</span>
          </div>
          <div className="stat">
            <span className="v">{(counts["corrupt"] ?? 0).toLocaleString()}</span>
            <span className="k">Corrupt</span>
          </div>
          <div className="stat">
            <span className="v">{(counts["unreadable"] ?? 0).toLocaleString()}</span>
            <span className="k">Unreadable</span>
          </div>
        </div>
      </div>

      {!locked && (
        <div className="card card-flat">
          <div className="history-toolbar">
            <h3 style={{ margin: 0, fontSize: "var(--text-lg)" }}>Saved libraries</h3>
            <div className="grow" />
            <button className="mini-btn" onClick={() => setEditing(newLibrary())}>
              <LibraryIcon /> New library
            </button>
          </div>

          {libraries.length === 0 ? (
            <p className="muted" style={{ margin: "var(--space-3) 0 0" }}>
              Save a folder set with its own encode target (movies vs. phone clips), then re-run it
              in one click. Add folders below and choose “Save as library”, or start a new one.
            </p>
          ) : (
            <div className="lib-list" style={{ marginTop: "var(--space-3)" }}>
              {libraries.map((lib) => (
                <div className="lib-row" key={lib.id}>
                  <div className="lib-row-main">
                    <span className="lib-name">{lib.name}</span>
                    <span className="muted lib-meta">
                      {lib.roots.length} folder{lib.roots.length === 1 ? "" : "s"} ·{" "}
                      {profileSummary(lib.profile)}
                    </span>
                  </div>
                  <div className="lib-row-actions">
                    <button
                      className="mini-btn"
                      onClick={() => runLibrary(lib)}
                      disabled={store.running || lib.roots.length === 0}
                      title="Encode this library with its profile"
                    >
                      <PlayIcon /> Run
                    </button>
                    <button
                      className="mini-btn"
                      onClick={() => scanLibrary(lib, false)}
                      disabled={scanning || lib.roots.length === 0}
                      title="Health-scan this library's folders"
                    >
                      <SearchIcon /> Scan
                    </button>
                    <button className="mini-btn" onClick={() => setEditing(lib)}>
                      Edit
                    </button>
                    <button className="mini-btn danger" onClick={() => removeLibrary(lib)}>
                      ✕
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {locked ? (
        <div className="card card-flat">
          <div className="empty">Scanning is disabled while the app is locked.</div>
        </div>
      ) : (
        <div className="card card-flat">
          <div className="history-toolbar">
            <button className="mini-btn" onClick={addFolders} disabled={scanning}>
              <FolderIcon /> Add folder
            </button>
            {roots.length > 0 && (
              <button
                className="mini-btn"
                onClick={saveCurrentAsLibrary}
                disabled={scanning}
                title="Save these folders and the current settings as a named library"
              >
                <LibraryIcon /> Save as library
              </button>
            )}
            <div className="grow" />
            {!scanning && flagged > 0 && (
              <span className="muted">
                {flagged} file{flagged === 1 ? "" : "s"} flagged
              </span>
            )}
            <button
              className="mini-btn"
              onClick={() => runScan(false)}
              disabled={scanning || roots.length === 0}
            >
              <SearchIcon /> Scan
            </button>
            <button
              className="mini-btn"
              onClick={() => runScan(true)}
              disabled={scanning || roots.length === 0}
              title="Decode each file to catch silent corruption (slower)"
            >
              <DeepScanIcon /> Deep scan
            </button>
          </div>

          {roots.length > 0 ? (
            <div className="queue" style={{ marginTop: "var(--space-3)" }}>
              {roots.map((r) => (
                <div className="queue-row" key={r}>
                  <span className="path" title={locked ? maskPath(r) : r}>
                    {maskPath(r)}
                  </span>
                  <button
                    className="rm"
                    onClick={() => removeRoot(r)}
                    disabled={scanning}
                    aria-label="Remove folder"
                  >
                    ✕
                  </button>
                </div>
              ))}
            </div>
          ) : (
            <p className="muted" style={{ margin: "var(--space-3) 0 0" }}>
              Add one or more folders, then Scan to check their health.
            </p>
          )}

          {scanning && (
            <div className="scan-progress">
              <div className="scan-progress-head">
                <span>{scanDeep ? "Deep scanning" : "Scanning"}…</span>
                <span className="muted">
                  {scanProgress
                    ? `${scanProgress.scanned} / ${scanProgress.total}${
                        scanProgress.total > 0 ? ` · ${pct(scanFrac)}` : ""
                      }`
                    : "preparing…"}
                </span>
              </div>
              <div className="bar" style={{ ["--p" as string]: scanFrac }}>
                <span />
              </div>
              {scanProgress && (
                <div
                  className="scan-progress-file muted"
                  title={locked ? undefined : scanProgress.path}
                >
                  {maskName(fileName(scanProgress.path))}
                </div>
              )}
            </div>
          )}

          {!scanning && scanError && (
            <p className="hw-note warn" style={{ marginTop: "var(--space-3)" }}>
              {scanError}
            </p>
          )}
        </div>
      )}

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
            {HEALTH_CHIPS.map((c) => {
              const n = counts[c.id] ?? 0;
              return (
                <button
                  key={c.id}
                  className="chip"
                  aria-pressed={filters.has(c.id)}
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
            {library
              ? allRows.length === 0
                ? "0 shown"
                : `${pageStart + 1}–${pageStart + pageRows.length} of ${allRows.length}`
              : "loading…"}
          </span>
          <div className="grow" />
          {!locked && allRows.length > 0 && (
            <button
              className="mini-btn danger"
              onClick={removeShown}
              title="Remove the shown files from the health list (keeps encode history)"
            >
              ✕ Remove {allRows.length} from library
            </button>
          )}
        </div>

        {library && allRows.length > 0 ? (
          pageRows.map((r) => {
            const h = healthMeta(r.health);
            const s = statusMeta(r.status);
            const actions = locked ? null : (
              <>
                <button className="mini-btn" onClick={() => openFile(r.path)}>
                  <PlayIcon /> Open
                </button>
                <button className="mini-btn" onClick={() => revealFile(r.path)}>
                  <FolderIcon /> Folder
                </button>
                <button className="mini-btn danger" onClick={() => removeOne(r.path)}>
                  ✕ Remove
                </button>
              </>
            );
            return (
              <StatusCard
                key={r.path}
                tone={h.tone}
                sym={h.sym}
                name={maskName(fileName(r.path))}
                fullPath={locked ? undefined : r.path}
                tag={h.label}
                meta={<span className="ecard-meta">{relativeTime(r.health_checked_at)}</span>}
                actions={actions}
              >
                <dl className="kv-grid">
                  <dt>path</dt>
                  <dd>{maskPath(r.path)}</dd>
                  <dt>health</dt>
                  <dd>{h.label}</dd>
                  {r.health_detail && (
                    <>
                      <dt>note</dt>
                      <dd>{r.health_detail}</dd>
                    </>
                  )}
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
                      <dt>size</dt>
                      <dd>{humanBytes(r.size)}</dd>
                    </>
                  )}
                  <dt>encode status</dt>
                  <dd>{s.label}</dd>
                  {r.health_checked_at && (
                    <>
                      <dt>last scan</dt>
                      <dd>{relativeTime(r.health_checked_at)}</dd>
                    </>
                  )}
                </dl>
              </StatusCard>
            );
          })
        ) : (
          <div className="empty">
            {library && library.rows.length === 0
              ? "No files scanned yet. Add a folder and Scan to check its health."
              : "Nothing matches."}
          </div>
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
      {editing && (
        <LibraryEditor
          initial={editing}
          onSave={saveLibrary}
          onClose={() => setEditing(null)}
        />
      )}
      {confirmModal}
    </div>
  );
}
