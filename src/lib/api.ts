// Thin typed wrappers over Tauri `invoke`.

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  Detection,
  FfStatus,
  History,
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
  isRunning: () => invoke<boolean>("is_running"),
  getHistory: () => invoke<History>("get_history"),
  getSettings: () => invoke<Record<string, unknown>>("get_settings"),
  saveSettings: (settings: Record<string, unknown>) =>
    invoke<void>("save_settings", { settings }),
};

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
