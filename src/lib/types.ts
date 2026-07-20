// Shared types mirroring the Rust command/event payloads.

export type Codec = "av1" | "hevc" | "h264";
export type QualityPreset =
  | "max-savings"
  | "balanced"
  | "high-quality"
  | "visually-lossless";
export type OnSuccess = "recycle" | "holding" | "delete";
export type EncoderFamily = "nvenc" | "amf" | "qsv" | "videotoolbox" | "software";
export type Container = "mkv" | "mp4";
export type AudioMode = "copy" | "opus" | "aac";
export type VerifyDepth = "fast" | "thorough" | "checksummed";
/** Pre-encode source health gate. `off` = legacy (probe failure is a plain
 *  failure). `structural` = classify+record health, skip unreadable sources
 *  (near-free). `deep` = also decode-probe the source to catch silent corruption. */
export type HealthGate = "off" | "structural" | "deep";
export type ScaleFilter = "lanczos" | "bicubic" | "bilinear" | "area";
export type BitDepth = "source" | "8" | "10";
export type EncoderSpeed =
  | "best"
  | "better"
  | "good"
  | "balanced"
  | "fast"
  | "faster"
  | "fastest";
export type Order =
  | "smart"
  | "largest-first"
  | "smallest-first"
  | "oldest-first"
  | "newest-first";

export interface Encoder {
  name: string;
  family: EncoderFamily;
}

export interface CodecSupport {
  codec: Codec;
  usable: Encoder[];
  selected: Encoder | null;
}

export interface Detection {
  codecs: CodecSupport[];
  has_hardware: boolean;
}

export interface FfStatus {
  present: boolean;
  ffmpeg: string;
  ffprobe: string;
  source: "none" | "custom" | "managed" | "system";
}

export interface FfmpegProgress {
  stage: "download" | "extract" | "done";
  downloaded: number;
  total: number;
}

export interface ScanResult {
  count: number;
  total_bytes: number;
}

/** One (codec × resolution) slice of a reclaimable-space projection. */
export interface ReclaimBucket {
  src_codec: string;
  height_band: string;
  files: number;
  candidate_bytes: number;
  est_reclaimable_bytes: number;
  est_skipped_files: number;
  sample_size: number;
  confidence: number;
}

/** How much a run would reclaim, estimated from the manifest's own history. */
export interface ReclaimProjection {
  /** 1 = instant estimate, 2 = probe-refined. */
  tier: number;
  candidate_files: number;
  candidate_bytes: number;
  est_reclaimable_bytes: number;
  est_skipped_files: number;
  buckets: ReclaimBucket[];
  based_on_history_rows: number;
  confidence: "low" | "fair" | "good";
  cold_start: boolean;
}

/** Subset of the Rust `Config` the UI sends (serde fills the rest). */
export interface RunConfig {
  inputs: string[];
  codec: Codec;
  quality: QualityPreset;
  quality_override?: number | null;
  encoder_override?: string | null;
  workers: number;
  min_savings: number;
  max_height: number;
  scale_filter: ScaleFilter;
  bit_depth: BitDepth;
  encoder_speed: EncoderSpeed;
  on_success: OnSuccess;
  holding_dir?: string | null;
  holding_retention_days: number;
  container: Container;
  audio_mode: AudioMode;
  audio_bitrate_kbps: number;
  verify_depth: VerifyDepth;
  /** Health-check each source before encoding (skip/flag unreadable or corrupt). */
  health_gate: HealthGate;
  ssim_floor?: number | null;
  /** VMAF quality mode: target a perceptual quality (0–100) instead of a fixed
   *  CRF. `null` = off (preset mode). */
  vmaf_target?: number | null;
  /** VMAF search sample count; 0 = auto (from resolution). Higher = slower/accurate. */
  vmaf_samples: number;
  /** VMAF search sample length in seconds; 0 = auto. Longer = slower/accurate. */
  vmaf_sample_secs: number;
  skip_dolby_vision: boolean;
  order: Order;
  paranoid: boolean;
  hardware_decode: boolean;
  dry_run: boolean;
  force: boolean;
  skip_marginal: boolean;
  marginal_bpp: number;
  early_abort: boolean;
  abort_stage1_at: number;
  abort_bloat_margin: number;
  abort_check_at: number;
  abort_late_at: number;
  abort_late_min_savings: number;
  retry_failed: boolean;
  normalize_container: boolean;
}

export type Outcome =
  | "done"
  | "normalized"
  | "skipped_efficient"
  | "skipped_marginal"
  | "skipped_no_gain"
  | "skipped_unhealthy"
  | "failed"
  | "cancelled"
  | "dry_run";

/** Manifest status strings (as stored in the DB). */
export type Status =
  | "indexed"
  | "pending"
  | "processing"
  | "done"
  | "normalized"
  | "skipped_already_efficient"
  | "skipped_marginal"
  | "skipped_no_gain"
  | "skipped_unhealthy"
  | "failed";

/** Per-file library health verdict (from a health scan). */
export type HealthState = "healthy" | "corrupt" | "unreadable";

/** A library entry: a known file with its encode status and health state. */
export interface LibraryRow {
  path: string;
  status: Status;
  size: number | null;
  src_codec: string | null;
  height: number | null;
  health: HealthState | null;
  health_detail: string | null;
  health_checked_at: number | null;
  updated_at: number | null;
}

/** The library view payload: counts by health + the file rows. */
export interface Library {
  /** Counts keyed by health state, with never-scanned files under "unscanned". */
  counts: Record<string, number>;
  rows: LibraryRow[];
}

/**
 * A named folder set with its own embedded encode profile. The re-runnable unit
 * 1.2.0's unattended mode binds to. `profile` is a `RunConfig` with `inputs`
 * cleared — running the library is `{ ...profile, inputs: roots }`.
 */
export interface SavedLibrary {
  id: string;
  name: string;
  roots: string[];
  profile: RunConfig;
  created_at: number;
  updated_at: number;
}

/** Tally returned when a health scan finishes. */
export interface HealthSummary {
  scanned: number;
  healthy: number;
  corrupt: number;
  unreadable: number;
  deep: boolean;
  cancelled: boolean;
}

/** Per-file health-scan progress event. */
export interface HealthProgress {
  scanned: number;
  total: number;
  path: string;
  health: HealthState;
}

export interface ProcessResult {
  path: string;
  outcome: Outcome;
  saved_bytes: number;
  message: string;
  orig_size: number | null;
  out_size: number | null;
}

export interface RunSummary {
  done: number;
  normalized: number;
  skipped: number;
  /** Sources the health gate rejected (unreadable/corrupt) and did not encode. */
  skipped_unhealthy: number;
  failed: number;
  would: number;
  saved_bytes: number;
  total_discovered: number;
  pending: number;
  processed: number;
  interrupted: boolean;
}

export interface HistoryRow {
  path: string;
  status: Status;
  size: number | null;
  src_codec: string | null;
  height: number | null;
  out_size: number | null;
  saved_bytes: number | null;
  error: string | null;
  /** Note when the encode succeeded only via a pipeline fallback (with reason). */
  fallback: string | null;
  updated_at: number | null;
}

export interface HistoryFilter {
  statuses?: string[];
  search?: string;
  limit?: number;
  offset?: number;
}

export interface History {
  total_reclaimed: number;
  encode_seconds: number;
  files_encoded: number;
  files_touched: number;
  bytes_in: number;
  bytes_out: number;
  counts: Record<string, number>;
  rows: HistoryRow[];
}

// Event payloads
export interface FileStart {
  path: string;
  name: string;
  duration: number | null;
  src_size: number;
}
export interface FileProgress {
  path: string;
  sec: number;
  out_bytes: number | null;
  fps: number | null;
  speed: number | null;
  bitrate_kbps: number | null;
}
export interface FileEnd {
  path: string;
}
/** Progress through the VMAF sample-encode search for a file (before its encode). */
export interface QualityProgress {
  path: string;
  /** Search progress, 0–1. */
  frac: number;
}
/** VMAF mode resolved a per-title CRF for a file (before its full encode). */
export interface QualityResolved {
  path: string;
  target: number;
  crf: number;
  /** Measured VMAF at `crf`, or null on a cache hit. */
  vmaf: number | null;
}

export interface LockStatus {
  /** A password has been set up at least once. */
  configured: boolean;
  /** The app is currently locked (masked + read-only). */
  locked: boolean;
}

export interface EnvInfo {
  os: string;
  arch: string;
  cpus: number;
  locale: string;
  ffmpeg_present: boolean;
  ffmpeg_path: string;
  ffmpeg_version: string | null;
  detection: Detection | null;
}
