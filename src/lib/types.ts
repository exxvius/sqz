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
  on_success: OnSuccess;
  holding_dir?: string | null;
  holding_retention_days: number;
  container: Container;
  audio_mode: AudioMode;
  audio_bitrate_kbps: number;
  verify_depth: VerifyDepth;
  ssim_floor?: number | null;
  skip_dolby_vision: boolean;
  order: Order;
  paranoid: boolean;
  hwaccel_decode: boolean;
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
  | "failed"
  | "cancelled"
  | "dry_run";

/** Manifest status strings (as stored in the DB). */
export type Status =
  | "pending"
  | "processing"
  | "done"
  | "normalized"
  | "skipped_already_efficient"
  | "skipped_marginal"
  | "skipped_no_gain"
  | "failed";

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
