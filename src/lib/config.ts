// UI-side default run config + persistence helpers.

import type { RunConfig } from "./types";

export const defaultConfig = (): RunConfig => ({
  inputs: [],
  codec: "av1",
  quality: "balanced",
  quality_override: null,
  encoder_override: null,
  workers: 2,
  min_savings: 0.1,
  max_height: 1080,
  scale_filter: "lanczos",
  bit_depth: "source",
  encoder_speed: "balanced",
  on_success: "recycle",
  holding_dir: null,
  holding_retention_days: 0,
  container: "mkv",
  audio_mode: "copy",
  audio_bitrate_kbps: 128,
  verify_depth: "fast",
  ssim_floor: null,
  vmaf_target: null,
  vmaf_samples: 0,
  vmaf_sample_secs: 0,
  skip_dolby_vision: true,
  order: "smart",
  paranoid: false,
  hardware_decode: true,
  dry_run: false,
  force: false,
  skip_marginal: false,
  marginal_bpp: 0.05,
  early_abort: true,
  abort_stage1_at: 0.05,
  abort_bloat_margin: 0.25,
  abort_check_at: 0.1,
  abort_late_at: 0.75,
  abort_late_min_savings: 0.03,
  retry_failed: true,
  normalize_container: false,
});

/** Everything except the transient input list is persisted between sessions. */
export function persistable(config: RunConfig): Record<string, unknown> {
  const { inputs: _inputs, ...rest } = config;
  return rest;
}

/** Merge persisted settings over the defaults, ignoring stale/unknown keys. */
export function fromPersisted(saved: Record<string, unknown>): RunConfig {
  const base = defaultConfig();
  const merged: Record<string, unknown> = { ...base };
  for (const key of Object.keys(base)) {
    if (key !== "inputs" && key in saved && saved[key] != null) {
      merged[key] = saved[key];
    }
  }
  return merged as unknown as RunConfig;
}
