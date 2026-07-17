// Thin typed wrappers over Tauri `invoke`.

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  Detection,
  FfStatus,
  History,
  HistoryFilter,
  RunConfig,
  ScanResult,
} from "./types";

export const api = {
  ffmpegStatus: () => invoke<FfStatus>("ffmpeg_status"),
  detectEncoders: () => invoke<Detection>("detect_encoders"),
  scanInputs: (inputs: string[]) => invoke<ScanResult>("scan_inputs", { inputs }),
  startRun: (config: RunConfig) => invoke<void>("start_run", { config }),
  pauseRun: () => invoke<void>("pause_run"),
  resumeRun: () => invoke<void>("resume_run"),
  cancelRun: () => invoke<void>("cancel_run"),
  abortFile: (path: string) => invoke<void>("abort_file", { path }),
  retryFile: (path: string) => invoke<void>("retry_file", { path }),
  forceFile: (path: string) => invoke<void>("force_file", { path }),
  isRunning: () => invoke<boolean>("is_running"),
  getHistory: (filter: HistoryFilter = {}) => invoke<History>("get_history", { filter }),
  deleteHistoryItem: (path: string) => invoke<void>("delete_history_item", { path }),
  deleteHistoryMatching: (filter: HistoryFilter) =>
    invoke<number>("delete_history_matching", { filter }),
  clearHistory: () => invoke<void>("clear_history"),
  getSettings: () => invoke<Record<string, unknown>>("get_settings"),
  saveSettings: (settings: Record<string, unknown>) =>
    invoke<void>("save_settings", { settings }),
  downloadFfmpeg: () => invoke<void>("download_ffmpeg"),
  setFfmpegPaths: (ffmpeg: string, ffprobe: string) =>
    invoke<void>("set_ffmpeg_paths", { ffmpeg, ffprobe }),
  clearFfmpegOverride: () => invoke<void>("clear_ffmpeg_override"),
};

/** Native picker for a single executable file (ffmpeg/ffprobe). */
export async function pickBinary(title: string): Promise<string | null> {
  const picked = await open({ multiple: false, directory: false, title });
  return typeof picked === "string" ? picked : null;
}

/** Open a file with the OS default application ("play"). */
export const openFile = (path: string) => invoke<void>("open_path", { path }).catch(() => {});
/** Reveal a file in its containing folder (file manager). */
export const revealFile = (path: string) => invoke<void>("reveal_path", { path }).catch(() => {});

/** Native picker for files and/or folders to add to the queue. */
export async function pickInputs(directory: boolean): Promise<string[]> {
  const picked = await open({
    multiple: true,
    directory,
    title: directory ? "Add folders" : "Add video files",
  });
  if (!picked) return [];
  return Array.isArray(picked) ? picked : [picked];
}
