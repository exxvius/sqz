// Shared types mirroring the Rust command/event payloads.

export type Codec = "av1" | "hevc" | "h264";
export type QualityPreset =
  | "max-savings"
  | "balanced"
  | "high-quality"
  | "visually-lossless";
export type OnSuccess = "recycle" | "holding" | "delete";
export type EncoderFamily = "nvenc" | "amf" | "qsv" | "videotoolbox" | "software";

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
  paranoid: boolean;
  hwaccel_decode: boolean;
  dry_run: boolean;
  force: boolean;
  skip_marginal: boolean;
}

export type Outcome =
  | "done"
  | "skipped_efficient"
  | "skipped_marginal"
  | "skipped_no_gain"
  | "failed"
  | "cancelled"
  | "dry_run";

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
  skipped_efficient: number;
  skipped_marginal: number;
  skipped_no_gain: number;
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
  src_codec: string | null;
  height: number | null;
  out_size: number | null;
  saved_bytes: number | null;
  updated_at: number | null;
}

export interface History {
  total_saved: number;
  counts: Record<string, number>;
  recent: HistoryRow[];
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
}
export interface FileEnd {
  path: string;
}
