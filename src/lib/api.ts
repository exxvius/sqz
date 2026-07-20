// Thin typed wrappers over Tauri `invoke`.

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  Detection,
  EnvInfo,
  FfStatus,
  HealthSummary,
  History,
  HistoryFilter,
  Library,
  LockStatus,
  ReclaimProjection,
  RunConfig,
  ScanResult,
} from "./types";

export const api = {
  ffmpegStatus: () => invoke<FfStatus>("ffmpeg_status"),
  detectEncoders: () => invoke<Detection>("detect_encoders"),
  scanInputs: (inputs: string[]) => invoke<ScanResult>("scan_inputs", { inputs }),
  projectReclaim: (config: RunConfig) =>
    invoke<ReclaimProjection>("project_reclaim", { config }),
  scanHealth: (config: RunConfig, deep: boolean) =>
    invoke<HealthSummary>("scan_health", { config, deep }),
  getLibrary: (filter: HistoryFilter = {}) => invoke<Library>("get_library", { filter }),
  deleteLibraryPaths: (paths: string[]) =>
    invoke<number>("delete_library_paths", { paths }),
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
  restoreOriginal: (path: string) => invoke<void>("restore_original", { path }),
  exportSettings: (dest: string) => invoke<void>("export_settings", { dest }),
  importSettings: (src: string) =>
    invoke<Record<string, unknown>>("import_settings", { src }),
  exportHistory: (dest: string, format: "csv" | "json", filter: HistoryFilter = {}) =>
    invoke<number>("export_history", { dest, format, filter }),
  environment: () => invoke<EnvInfo>("environment"),
  quitApp: () => invoke<void>("quit_app"),
  lockStatus: () => invoke<LockStatus>("lock_status"),
  lockSetup: (password: string) => invoke<void>("lock_setup", { password }),
  lockApp: () => invoke<void>("lock_app"),
  unlockApp: (password: string) => invoke<void>("unlock_app", { password }),
  lockChangePassword: (oldPassword: string, newPassword: string) =>
    invoke<void>("lock_change_password", { oldPassword, newPassword }),
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
