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
  on_success: "recycle",
  holding_dir: null,
  paranoid: false,
  hwaccel_decode: false,
  dry_run: false,
  force: false,
  skip_marginal: false,
});

/** Persisted settings are a subset of RunConfig used as form defaults. */
export type PersistedDefaults = Pick<
  RunConfig,
  "codec" | "quality" | "workers" | "min_savings" | "max_height" | "on_success" | "skip_marginal"
>;

export function applyDefaults(base: RunConfig, saved: Partial<PersistedDefaults>): RunConfig {
  return { ...base, ...saved };
}
