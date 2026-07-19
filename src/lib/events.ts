// Event name constants + typed listeners. Names mirror src-tauri/src/events.rs.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FileEnd, FileProgress, FileStart, ProcessResult, RunSummary } from "./types";

export const EV = {
  fileStart: "sqz-file-start",
  fileProgress: "sqz-file-progress",
  fileEnd: "sqz-file-end",
  fileRecord: "sqz-file-record",
  runStart: "sqz-run-start",
  runDone: "sqz-run-done",
  projection: "sqz-projection",
} as const;

export interface EngineHandlers {
  onFileStart?: (p: FileStart) => void;
  onFileProgress?: (p: FileProgress) => void;
  onFileEnd?: (p: FileEnd) => void;
  onRecord?: (p: ProcessResult) => void;
  onRunStart?: (total: number) => void;
  onRunDone?: (p: RunSummary) => void;
}

/** Subscribe to all engine events; returns a single unlisten function. */
export async function subscribeEngine(h: EngineHandlers): Promise<UnlistenFn> {
  const unlisteners: UnlistenFn[] = await Promise.all([
    listen<FileStart>(EV.fileStart, (e) => h.onFileStart?.(e.payload)),
    listen<FileProgress>(EV.fileProgress, (e) => h.onFileProgress?.(e.payload)),
    listen<FileEnd>(EV.fileEnd, (e) => h.onFileEnd?.(e.payload)),
    listen<ProcessResult>(EV.fileRecord, (e) => h.onRecord?.(e.payload)),
    listen<{ total: number }>(EV.runStart, (e) => h.onRunStart?.(e.payload.total)),
    listen<RunSummary>(EV.runDone, (e) => h.onRunDone?.(e.payload)),
  ]);
  return () => unlisteners.forEach((u) => u());
}
