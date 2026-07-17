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
import type { ProcessResult, RunConfig, RunSummary } from "./types";

export interface ActiveFile {
  path: string;
  name: string;
  duration: number | null;
  srcSize: number;
  sec: number;
  outBytes: number | null;
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
  active: Record<string, ActiveFile>;
  log: LogEntry[];
  summary: RunSummary | null;
  session: {
    saved: number;
    done: number;
    failed: number;
    skipped: number;
    processed: number;
  };
}

type Action =
  | { type: "SET_RUNNING"; running: boolean }
  | { type: "SET_PAUSED"; paused: boolean }
  | { type: "RUN_START"; minSavings: number }
  | { type: "FILE_START"; path: string; name: string; duration: number | null; srcSize: number }
  | { type: "FILE_PROGRESS"; path: string; sec: number; outBytes: number | null }
  | { type: "FILE_END"; path: string }
  | { type: "RECORD"; result: ProcessResult }
  | { type: "RUN_DONE"; summary: RunSummary };

const LOG_CAP = 500;

const initial: State = {
  running: false,
  paused: false,
  minSavings: 0.1,
  active: {},
  log: [],
  summary: null,
  session: { saved: 0, done: 0, failed: 0, skipped: 0, processed: 0 },
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
        session: { saved: 0, done: 0, failed: 0, skipped: 0, processed: 0 },
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
          },
        },
      };
    case "FILE_PROGRESS": {
      const f = state.active[action.path];
      if (!f) return state;
      return {
        ...state,
        active: {
          ...state.active,
          [action.path]: { ...f, sec: action.sec, outBytes: action.outBytes },
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
      session.processed += 1;
      if (r.outcome === "done") {
        session.done += 1;
        session.saved += r.saved_bytes;
      } else if (r.outcome === "failed") {
        session.failed += 1;
      } else if (r.outcome.startsWith("skipped")) {
        session.skipped += 1;
      }
      return { ...state, log: [entry, ...state.log].slice(0, LOG_CAP), session };
    }
    case "RUN_DONE":
      return { ...state, running: false, paused: false, summary: action.summary };
    default:
      return state;
  }
}

interface StoreValue extends State {
  start: (config: RunConfig) => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  cancel: () => Promise<void>;
}

const StoreContext = createContext<StoreValue | null>(null);

export function StoreProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initial);
  const pendingMinSavings = useRef(0.1);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    subscribeEngine({
      onRunStart: () =>
        dispatch({ type: "RUN_START", minSavings: pendingMinSavings.current }),
      onFileStart: (p) =>
        dispatch({
          type: "FILE_START",
          path: p.path,
          name: p.name,
          duration: p.duration,
          srcSize: p.src_size,
        }),
      onFileProgress: (p) =>
        dispatch({ type: "FILE_PROGRESS", path: p.path, sec: p.sec, outBytes: p.out_bytes }),
      onFileEnd: (p) => dispatch({ type: "FILE_END", path: p.path }),
      onRecord: (r) => dispatch({ type: "RECORD", result: r }),
      onRunDone: (s) => dispatch({ type: "RUN_DONE", summary: s }),
    }).then((u) => (unlisten = u));

    // Reconcile in case a run is already active (e.g. window reopened).
    api.isRunning().then((running) => dispatch({ type: "SET_RUNNING", running }));

    return () => unlisten?.();
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
