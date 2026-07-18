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
import { subscribeEngine } from "./events";
import type { FileProgress, ProcessResult, RunConfig, RunSummary } from "./types";

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
}

type Action =
  | { type: "SET_RUNNING"; running: boolean }
  | { type: "SET_PAUSED"; paused: boolean }
  | { type: "RUN_START"; minSavings: number; total: number }
  | { type: "FILE_START"; path: string; name: string; duration: number | null; srcSize: number }
  | { type: "FILE_PROGRESS"; p: FileProgress }
  | { type: "FILE_END"; path: string }
  | { type: "RECORD"; result: ProcessResult }
  | { type: "RUN_DONE"; summary: RunSummary }
  | { type: "CLEAR_LOG" };

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
          },
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
      onFileEnd: (p) => dispatch({ type: "FILE_END", path: p.path }),
      onRecord: (r) => dispatch({ type: "RECORD", result: r }),
      onRunDone: (s) => dispatch({ type: "RUN_DONE", summary: s }),
    }).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });

    api.isRunning().then((running) => dispatch({ type: "SET_RUNNING", running }));

    return () => {
      disposed = true;
      unlisten?.();
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
