// Central run store: subscribes to engine events and exposes live state +
// actions to the whole app via context.

import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  type ReactNode,
} from "react";
import { api } from "./api";
import { subscribeEngine, subscribeHealth } from "./events";
import type {
  FileProgress,
  HealthProgress,
  ProcessResult,
  QualityProgress,
  QualityResolved,
  RunConfig,
  RunSummary,
} from "./types";

const PROJECTION_HISTORY = 24;

export interface ActiveFile {
  path: string;
  name: string;
  duration: number | null;
  srcSize: number;
  sec: number;
  outBytes: number | null;
  fps: number | null;
  speed: number | null;
  bitrateKbps: number | null;
  /** Recent projected final sizes, for the trend indicator. */
  projections: number[];
  /** VMAF-mode quality resolution, e.g. "VMAF 95 → CRF 32". Null in preset mode. */
  qualityNote: string | null;
  /** VMAF sample-search progress (0–1) before the real encode; null when not
   *  searching (preset mode, or once the encode has started). */
  searchFrac: number | null;
  /** Wall-clock ms when the search started (first progress tick). */
  searchStartedAt: number | null;
  /** Smoothed estimated seconds remaining for the search, or null until stable. */
  searchEta: number | null;
}

export interface LogEntry {
  path: string;
  name: string;
  outcome: ProcessResult["outcome"];
  message: string;
  savedBytes: number;
  origSize: number | null;
  outSize: number | null;
  at: number;
}

interface State {
  running: boolean;
  paused: boolean;
  minSavings: number;
  queueTotal: number;
  active: Record<string, ActiveFile>;
  log: LogEntry[];
  summary: RunSummary | null;
  session: {
    saved: number;
    done: number;
    normalized: number;
    failed: number;
    skipped: number;
    processed: number;
  };
  /** Library health scan — lifted here so its progress survives tab switches. */
  scanning: boolean;
  scanDeep: boolean;
  scanProgress: HealthProgress | null;
  scanError: string | null;
}

type Action =
  | { type: "SET_RUNNING"; running: boolean }
  | { type: "SET_PAUSED"; paused: boolean }
  | { type: "RUN_START"; minSavings: number; total: number }
  | { type: "FILE_START"; path: string; name: string; duration: number | null; srcSize: number }
  | { type: "FILE_PROGRESS"; p: FileProgress }
  | { type: "QUALITY_PROGRESS"; p: QualityProgress }
  | { type: "QUALITY_RESOLVED"; p: QualityResolved }
  | { type: "FILE_END"; path: string }
  | { type: "RECORD"; result: ProcessResult }
  | { type: "RUN_DONE"; summary: RunSummary }
  | { type: "CLEAR_LOG" }
  | { type: "SCAN_START"; deep: boolean }
  | { type: "SCAN_PROGRESS"; p: HealthProgress }
  | { type: "SCAN_END" }
  | { type: "SCAN_FAIL"; error: string };

const LOG_CAP = 1000;

const emptySession = () => ({
  saved: 0,
  done: 0,
  normalized: 0,
  failed: 0,
  skipped: 0,
  processed: 0,
});

const initial: State = {
  running: false,
  paused: false,
  minSavings: 0.1,
  queueTotal: 0,
  active: {},
  log: [],
  summary: null,
  session: emptySession(),
  scanning: false,
  scanDeep: false,
  scanProgress: null,
  scanError: null,
};

function nameOf(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "SET_RUNNING":
      return { ...state, running: action.running };
    case "SET_PAUSED":
      return { ...state, paused: action.paused };
    case "RUN_START":
      return {
        ...state,
        running: true,
        paused: false,
        summary: null,
        active: {},
        minSavings: action.minSavings,
        queueTotal: action.total,
        session: emptySession(),
      };
    case "FILE_START":
      return {
        ...state,
        active: {
          ...state.active,
          [action.path]: {
            path: action.path,
            name: action.name,
            duration: action.duration,
            srcSize: action.srcSize,
            sec: 0,
            outBytes: null,
            fps: null,
            speed: null,
            bitrateKbps: null,
            projections: [],
            qualityNote: null,
            searchFrac: null,
            searchStartedAt: null,
            searchEta: null,
          },
        },
      };
    case "FILE_PROGRESS": {
      const f = state.active[action.p.path];
      if (!f) return state;
      const projections = f.projections.slice();
      if (action.p.out_bytes && f.duration && f.duration > 0) {
        const frac = action.p.sec / f.duration;
        if (frac > 0) {
          projections.push(action.p.out_bytes / frac);
          if (projections.length > PROJECTION_HISTORY) projections.shift();
        }
      }
      return {
        ...state,
        active: {
          ...state.active,
          [action.p.path]: {
            ...f,
            sec: action.p.sec,
            outBytes: action.p.out_bytes,
            fps: action.p.fps,
            speed: action.p.speed,
            bitrateKbps: action.p.bitrate_kbps,
            projections,
            // A real encode tick means the search is over; hand the bar back.
            searchFrac: null,
            searchEta: null,
          },
        },
      };
    }
    case "QUALITY_PROGRESS": {
      const f = state.active[action.p.path];
      if (!f) return state;
      // Hold just under 1 so the bar reads as "still working" until the encode
      // takes over — it never implies the search is 100% done.
      const frac = Math.min(action.p.frac, 0.99);
      const now = Date.now();
      const startedAt = f.searchStartedAt ?? now;
      const elapsed = (now - startedAt) / 1000;
      // ETA from the observed rate of progress (work is ~uniform per fraction).
      // Only trust it once there's a little signal, and smooth it (EMA) so it
      // doesn't jitter frame-to-frame.
      let eta = f.searchEta;
      if (frac > 0.06 && elapsed > 3) {
        const raw = (elapsed * (1 - frac)) / frac;
        eta = f.searchEta == null ? raw : f.searchEta * 0.6 + raw * 0.4;
      }
      return {
        ...state,
        active: {
          ...state.active,
          [action.p.path]: { ...f, searchFrac: frac, searchStartedAt: startedAt, searchEta: eta },
        },
      };
    }
    case "QUALITY_RESOLVED": {
      const f = state.active[action.p.path];
      if (!f) return state;
      const { target, crf, vmaf } = action.p;
      const note =
        vmaf != null
          ? `VMAF ${target} → CRF ${crf} (${vmaf.toFixed(1)})`
          : `VMAF ${target} → CRF ${crf} (cached)`;
      // Search done; the real encode is about to drive the bar.
      return {
        ...state,
        active: {
          ...state.active,
          [action.p.path]: { ...f, qualityNote: note, searchFrac: null, searchEta: null },
        },
      };
    }
    case "FILE_END": {
      if (!state.active[action.path]) return state;
      const next = { ...state.active };
      delete next[action.path];
      return { ...state, active: next };
    }
    case "RECORD": {
      const r = action.result;
      const entry: LogEntry = {
        path: r.path,
        name: nameOf(r.path),
        outcome: r.outcome,
        message: r.message,
        savedBytes: r.saved_bytes,
        origSize: r.orig_size,
        outSize: r.out_size,
        at: Date.now(),
      };
      const session = { ...state.session };
      if (r.outcome !== "cancelled") session.processed += 1;
      if (r.outcome === "done") {
        session.done += 1;
        session.saved += r.saved_bytes;
      } else if (r.outcome === "normalized") {
        session.normalized += 1;
        if (r.saved_bytes > 0) session.saved += r.saved_bytes;
      } else if (r.outcome === "failed") {
        session.failed += 1;
      } else if (r.outcome.startsWith("skipped")) {
        session.skipped += 1;
      }
      return { ...state, log: [entry, ...state.log].slice(0, LOG_CAP), session };
    }
    case "RUN_DONE":
      return { ...state, running: false, paused: false, summary: action.summary };
    case "CLEAR_LOG":
      // Visually clears the on-screen event log only — the manifest DB (History
      // tab) is untouched.
      return { ...state, log: [] };
    case "SCAN_START":
      return { ...state, scanning: true, scanDeep: action.deep, scanProgress: null, scanError: null };
    case "SCAN_PROGRESS":
      // A late progress tick from a finished/failed scan must not revive the bar.
      return state.scanning ? { ...state, scanProgress: action.p } : state;
    case "SCAN_END":
      return { ...state, scanning: false, scanProgress: null };
    case "SCAN_FAIL":
      return { ...state, scanning: false, scanProgress: null, scanError: action.error };
    default:
      return state;
  }
}

interface StoreValue extends State {
  start: (config: RunConfig) => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  cancel: () => Promise<void>;
  abortFile: (path: string) => Promise<void>;
  retryFile: (path: string) => Promise<void>;
  forceFile: (path: string) => Promise<void>;
  clearLog: () => void;
  /** Start a library health scan; progress lands in `scanProgress`. */
  startScan: (config: RunConfig, deep: boolean) => Promise<void>;
}

const StoreContext = createContext<StoreValue | null>(null);

export function StoreProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initial);
  const pendingMinSavings = useRef(0.1);

  useEffect(() => {
    // Guard against React StrictMode's mount/cleanup/mount: if the async
    // subscription resolves after this effect was already cleaned up, unlisten
    // immediately so we never end up with two live listeners (duplicate events).
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenHealth: (() => void) | undefined;
    subscribeEngine({
      onRunStart: (total) =>
        dispatch({ type: "RUN_START", minSavings: pendingMinSavings.current, total }),
      onFileStart: (p) =>
        dispatch({
          type: "FILE_START",
          path: p.path,
          name: p.name,
          duration: p.duration,
          srcSize: p.src_size,
        }),
      onFileProgress: (p) => dispatch({ type: "FILE_PROGRESS", p }),
      onQualityProgress: (p) => dispatch({ type: "QUALITY_PROGRESS", p }),
      onQualityResolved: (p) => dispatch({ type: "QUALITY_RESOLVED", p }),
      onFileEnd: (p) => dispatch({ type: "FILE_END", path: p.path }),
      onRecord: (r) => dispatch({ type: "RECORD", result: r }),
      onRunDone: (s) => dispatch({ type: "RUN_DONE", summary: s }),
    }).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });

    // Health-scan events feed the shared scan state, so the Library progress bar
    // keeps updating even while another tab is showing.
    subscribeHealth({
      onProgress: (p) => dispatch({ type: "SCAN_PROGRESS", p }),
      onDone: () => dispatch({ type: "SCAN_END" }),
    }).then((u) => {
      if (disposed) u();
      else unlistenHealth = u;
    });

    api.isRunning().then((running) => dispatch({ type: "SET_RUNNING", running }));

    return () => {
      disposed = true;
      unlisten?.();
      unlistenHealth?.();
    };
  }, []);

  const value = useMemo<StoreValue>(
    () => ({
      ...state,
      start: async (config) => {
        pendingMinSavings.current = config.min_savings;
        await api.startRun(config);
      },
      pause: async () => {
        await api.pauseRun();
        dispatch({ type: "SET_PAUSED", paused: true });
      },
      resume: async () => {
        await api.resumeRun();
        dispatch({ type: "SET_PAUSED", paused: false });
      },
      cancel: async () => {
        await api.cancelRun();
      },
      abortFile: (path) => api.abortFile(path),
      retryFile: (path) => api.retryFile(path),
      forceFile: (path) => api.forceFile(path),
      clearLog: () => dispatch({ type: "CLEAR_LOG" }),
      startScan: async (config, deep) => {
        if (state.scanning) return;
        dispatch({ type: "SCAN_START", deep });
        try {
          await api.scanHealth(config, deep);
          dispatch({ type: "SCAN_END" });
        } catch (e) {
          dispatch({ type: "SCAN_FAIL", error: e instanceof Error ? e.message : "Scan failed." });
        }
      },
    }),
    [state],
  );

  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}

export function useStore(): StoreValue {
  const ctx = useContext(StoreContext);
  if (!ctx) throw new Error("useStore must be used within StoreProvider");
  return ctx;
}
