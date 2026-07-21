// Event name constants + typed listeners. Names mirror src-tauri/src/events.rs.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  FileEnd,
  FileProgress,
  FileStart,
  HealthProgress,
  HealthSummary,
  ProcessResult,
  QualityProgress,
  QualityResolved,
  RunSourceInfo,
  RunSummary,
} from "./types";

export const EV = {
  fileStart: "sqz-file-start",
  fileProgress: "sqz-file-progress",
  fileEnd: "sqz-file-end",
  fileRecord: "sqz-file-record",
  runStart: "sqz-run-start",
  runDone: "sqz-run-done",
  qualityProgress: "sqz-quality-progress",
  qualityResolved: "sqz-quality-resolved",
  gateProgress: "sqz-gate-progress",
  projection: "sqz-projection",
  healthProgress: "sqz-health-progress",
  healthDone: "sqz-health-done",
  runSource: "sqz-run-source",
  runPaused: "sqz-run-paused",
} as const;

/** Subscribe to health-scan progress + completion; returns an unlisten fn. */
export async function subscribeHealth(handlers: {
  onProgress?: (p: HealthProgress) => void;
  onDone?: (s: HealthSummary) => void;
}): Promise<UnlistenFn> {
  const unlisteners: UnlistenFn[] = await Promise.all([
    listen<HealthProgress>(EV.healthProgress, (e) =>
      handlers.onProgress?.(e.payload),
    ),
    listen<HealthSummary>(EV.healthDone, (e) => handlers.onDone?.(e.payload)),
  ]);
  return () => unlisteners.forEach((u) => u());
}

export interface EngineHandlers {
  onFileStart?: (p: FileStart) => void;
  onFileProgress?: (p: FileProgress) => void;
  onFileEnd?: (p: FileEnd) => void;
  onQualityProgress?: (p: QualityProgress) => void;
  onQualityResolved?: (p: QualityResolved) => void;
  /** Health-gate (Deep) source-decode progress, before the encode. Same
   *  `{ path, frac }` shape as a quality-search tick. */
  onGateProgress?: (p: QualityProgress) => void;
  onRecord?: (p: ProcessResult) => void;
  onRunStart?: (total: number) => void;
  onRunDone?: (p: RunSummary) => void;
  /** A run launched: manual, or an unattended (scheduled) run of a library. */
  onRunSource?: (info: RunSourceInfo) => void;
  /** The supervisor auto-paused/resumed an unattended run on machine activity. */
  onRunPaused?: (paused: boolean) => void;
}

/** Subscribe to all engine events; returns a single unlisten function. */
export async function subscribeEngine(h: EngineHandlers): Promise<UnlistenFn> {
  const unlisteners: UnlistenFn[] = await Promise.all([
    listen<FileStart>(EV.fileStart, (e) => h.onFileStart?.(e.payload)),
    listen<FileProgress>(EV.fileProgress, (e) => h.onFileProgress?.(e.payload)),
    listen<FileEnd>(EV.fileEnd, (e) => h.onFileEnd?.(e.payload)),
    listen<QualityProgress>(EV.qualityProgress, (e) =>
      h.onQualityProgress?.(e.payload),
    ),
    listen<QualityResolved>(EV.qualityResolved, (e) =>
      h.onQualityResolved?.(e.payload),
    ),
    listen<QualityProgress>(EV.gateProgress, (e) =>
      h.onGateProgress?.(e.payload),
    ),
    listen<ProcessResult>(EV.fileRecord, (e) => h.onRecord?.(e.payload)),
    listen<{ total: number }>(EV.runStart, (e) =>
      h.onRunStart?.(e.payload.total),
    ),
    listen<RunSummary>(EV.runDone, (e) => h.onRunDone?.(e.payload)),
    listen<RunSourceInfo>(EV.runSource, (e) => h.onRunSource?.(e.payload)),
    listen<{ paused: boolean }>(EV.runPaused, (e) =>
      h.onRunPaused?.(e.payload.paused),
    ),
  ]);
  return () => unlisteners.forEach((u) => u());
}
