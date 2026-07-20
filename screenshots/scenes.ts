// Canned IPC responses + scripted engine-event scenes for screenshotting the
// real UI in a headless browser (no Tauri backend). Shapes mirror src/lib/types.

import { emit } from "@tauri-apps/api/event";

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const TB = 1024 ** 4;
const now = Math.floor(Date.now() / 1000);

const enc = (name: string, family: string) => ({ name, family });

const DETECTION = {
  has_hardware: true,
  codecs: [
    {
      codec: "av1",
      usable: [enc("av1_nvenc", "nvenc"), enc("libsvtav1", "software")],
      selected: enc("av1_nvenc", "nvenc"),
    },
    {
      codec: "hevc",
      usable: [enc("hevc_nvenc", "nvenc"), enc("libx265", "software")],
      selected: enc("hevc_nvenc", "nvenc"),
    },
    {
      codec: "h264",
      usable: [enc("h264_nvenc", "nvenc"), enc("libx264", "software")],
      selected: enc("h264_nvenc", "nvenc"),
    },
  ],
};

const HISTORY = {
  total_reclaimed: Math.round(419.7 * GB),
  encode_seconds: 235800, // ~65h 30m
  files_encoded: 214,
  files_touched: 317,
  bytes_in: Math.round(723.6 * GB),
  bytes_out: Math.round(303.9 * GB),
  counts: {
    done: 214,
    normalized: 12,
    failed: 3,
    skipped_no_gain: 8,
    skipped_already_efficient: 61,
    skipped_marginal: 19,
    pending: 0,
    processing: 0,
  },
  rows: [
    {
      path: "D:\\Media\\Movies\\Blade Runner 2049 (2017) 2160p HDR.mkv",
      status: "done",
      size: Math.round(38.4 * GB),
      src_codec: "h264",
      height: 2160,
      out_size: Math.round(15.1 * GB),
      saved_bytes: Math.round(23.3 * GB),
      error: null,
      updated_at: now - 240,
    },
    {
      path: "D:\\Media\\Movies\\Dune Part Two (2024) 2160p.mkv",
      status: "done",
      size: Math.round(41.2 * GB),
      src_codec: "hevc",
      height: 2160,
      out_size: Math.round(17.6 * GB),
      saved_bytes: Math.round(23.6 * GB),
      error: null,
      updated_at: now - 900,
    },
    {
      path: "D:\\Media\\TV\\The Expanse\\S03E09 - Intransigence 1080p.mkv",
      status: "done",
      size: Math.round(4.7 * GB),
      src_codec: "h264",
      height: 1080,
      out_size: Math.round(1.9 * GB),
      saved_bytes: Math.round(2.8 * GB),
      error: null,
      updated_at: now - 3600 * 2,
    },
    {
      path: "D:\\Media\\Home\\Wedding 2011 (camcorder).avi",
      status: "normalized",
      size: Math.round(1.24 * GB),
      src_codec: "mpeg4",
      height: 720,
      out_size: Math.round(1.19 * GB),
      saved_bytes: Math.round(0.05 * GB),
      error: null,
      updated_at: now - 3600 * 5,
    },
    {
      path: "D:\\Media\\TV\\Arcane\\S01E03 1080p HEVC.mkv",
      status: "skipped_already_efficient",
      size: Math.round(2.1 * GB),
      src_codec: "hevc",
      height: 1080,
      out_size: null,
      saved_bytes: 0,
      error: null,
      updated_at: now - 3600 * 26,
    },
    {
      path: "D:\\Media\\Movies\\Sintel (2010) 1080p.mkv",
      status: "done",
      size: Math.round(1.12 * GB),
      src_codec: "h264",
      height: 1080,
      out_size: Math.round(438 * MB),
      saved_bytes: Math.round(0.69 * GB),
      error: null,
      updated_at: now - 3600 * 27,
    },
    {
      path: "D:\\Media\\Movies\\Old Transfer (2003).mkv",
      status: "skipped_no_gain",
      size: Math.round(1.6 * GB),
      src_codec: "h264",
      height: 576,
      out_size: null,
      saved_bytes: 0,
      error: null,
      updated_at: now - 3600 * 30,
    },
    {
      path: "D:\\Media\\Recordings\\Capture 2023-11-02.mkv",
      status: "failed",
      size: Math.round(6.0 * GB),
      src_codec: null,
      height: null,
      out_size: null,
      saved_bytes: null,
      error: "ffprobe: moov atom not found — file is truncated or still being written",
      updated_at: now - 3600 * 33,
    },
    {
      path: "D:\\Media\\TV\\Planet Earth II\\S01E02 - Mountains 2160p.mkv",
      status: "done",
      size: Math.round(22.8 * GB),
      src_codec: "h264",
      height: 2160,
      out_size: Math.round(9.4 * GB),
      saved_bytes: Math.round(13.4 * GB),
      error: null,
      updated_at: now - 3600 * 40,
    },
  ],
};

// A real library has hundreds of files; pad the curated rows above out to match
// the status counts, so the filter pills, the paging total, and the stats all
// agree (and the list shows realistic pagination) rather than "1–9 of 9".
type Row = {
  path: string;
  status: string;
  size: number | null;
  src_codec: string | null;
  height: number | null;
  out_size: number | null;
  saved_bytes: number | null;
  error: string | null;
  updated_at: number;
};

const SHOWS = [
  "Breaking Bad", "The Expanse", "Chernobyl", "Fargo", "Severance", "Andor",
  "The Bear", "Succession", "Better Call Saul", "Foundation", "Silo", "Cosmos",
];
const GEN_RES = [720, 1080, 1080, 1080, 1440, 2160];
let seq = 0;

function sizeFor(h: number, i: number): number {
  const base = h >= 2160 ? 26 : h >= 1440 ? 10 : h >= 1080 ? 3.8 : 1.3; // GB
  return Math.round(base * (0.8 + (i % 6) * 0.09) * GB);
}

// One generated TV-episode row. Season/episode derive uniquely from `seq`, so
// every path is distinct (React-key safe) and updated_at strictly decreases.
function genRow(status: string): Row {
  const show = SHOWS[seq % SHOWS.length];
  const season = 1 + Math.floor(seq / 24);
  const ep = 1 + (seq % 24);
  const height = GEN_RES[seq % GEN_RES.length];
  const s = String(season).padStart(2, "0");
  const e = String(ep).padStart(2, "0");
  const path = `D:\\Media\\TV\\${show}\\S${s}E${e} ${height}p.mkv`;
  const updated_at = now - 3600 * 3 - seq * 1500;
  const size = sizeFor(height, seq);
  seq += 1;
  const base: Row = {
    path, status, height, updated_at,
    size, src_codec: null, out_size: null, saved_bytes: null, error: null,
  };
  switch (status) {
    case "done": {
      const codec = height >= 1080 ? (seq % 3 === 0 ? "hevc" : "h264") : "mpeg2video";
      const out = Math.round(size * (0.42 + (seq % 4) * 0.03));
      return { ...base, src_codec: codec, out_size: out, saved_bytes: size - out };
    }
    case "normalized": {
      const out = Math.round(size * 0.98);
      return { ...base, src_codec: "mpeg4", out_size: out, saved_bytes: size - out };
    }
    case "failed":
      return {
        ...base, size, height: null, src_codec: null,
        error: "ffprobe: moov atom not found — file is truncated or still being written",
      };
    case "skipped_already_efficient":
      return { ...base, src_codec: seq % 2 ? "hevc" : "av1", saved_bytes: 0 };
    default: // skipped_no_gain, skipped_marginal
      return { ...base, src_codec: "h264", saved_bytes: 0 };
  }
}

function padRows(n: number, status: string) {
  for (let i = 0; i < n; i += 1) (HISTORY.rows as Row[]).push(genRow(status));
}

// Counts above already sum to files_touched (317); pad each status to match.
padRows(209, "done");
padRows(11, "normalized");
padRows(2, "failed");
padRows(7, "skipped_no_gain");
padRows(60, "skipped_already_efficient");
padRows(19, "skipped_marginal");
(HISTORY.rows as Row[]).sort((a, b) => b.updated_at - a.updated_at);

// Folders returned by the mocked native picker on the Home scene (folders, so
// the discovered-video count in the action bar is consistent with the sources).
const HOME_INPUTS = [
  "D:\\Media\\Movies",
  "D:\\Media\\TV",
  "D:\\Media\\Home Videos",
  "D:\\Media\\Concerts",
];

// Reclaimable-space projection for the Home scene. Tier 1 lands instantly from
// project_reclaim; Tier 2 (with the per-bucket breakdown) is emitted right after
// via the sqz-projection event, mirroring the real backend.
const HOME_BUCKETS = [
  { src_codec: "h264", height_band: "2160p", files: 42, candidate_bytes: Math.round(980 * GB), est_reclaimable_bytes: Math.round(610 * GB), est_skipped_files: 0, sample_size: 46, confidence: 0.7 },
  { src_codec: "hevc", height_band: "2160p", files: 26, candidate_bytes: Math.round(720 * GB), est_reclaimable_bytes: Math.round(300 * GB), est_skipped_files: 0, sample_size: 33, confidence: 0.62 },
  { src_codec: "h264", height_band: "1080p", files: 44, candidate_bytes: Math.round(430 * GB), est_reclaimable_bytes: Math.round(250 * GB), est_skipped_files: 2, sample_size: 128, confidence: 0.86 },
  { src_codec: "h264", height_band: "1440p", files: 14, candidate_bytes: Math.round(210 * GB), est_reclaimable_bytes: Math.round(124 * GB), est_skipped_files: 0, sample_size: 18, confidence: 0.47 },
  { src_codec: "mpeg4", height_band: "≤720p", files: 6, candidate_bytes: Math.round(60 * GB), est_reclaimable_bytes: Math.round(42 * GB), est_skipped_files: 0, sample_size: 7, confidence: 0.26 },
  { src_codec: "hevc", height_band: "1080p", files: 0, candidate_bytes: 0, est_reclaimable_bytes: 0, est_skipped_files: 6, sample_size: 40, confidence: 0.67 },
];
const HOME_PROJECTION_T2 = {
  tier: 2,
  candidate_files: 140,
  candidate_bytes: Math.round(2.54 * TB),
  est_reclaimable_bytes: Math.round(1326 * GB),
  est_skipped_files: 8,
  buckets: HOME_BUCKETS,
  based_on_history_rows: 214,
  confidence: "good",
  cold_start: false,
};
const HOME_PROJECTION_T1 = {
  ...HOME_PROJECTION_T2,
  tier: 1,
  est_reclaimable_bytes: Math.round(1.4 * TB),
  est_skipped_files: 0,
  buckets: [],
};

/** Handle a mocked `invoke` for a given scene. */
export function commandHandler(
  cmd: string,
  args: any,
  scene: string,
  opts?: { locked?: boolean },
): unknown {
  switch (cmd) {
    case "ffmpeg_status":
      return {
        present: true,
        ffmpeg: "C:\\Users\\you\\AppData\\Roaming\\sqz\\ffmpeg\\ffmpeg.exe",
        ffprobe: "C:\\Users\\you\\AppData\\Roaming\\sqz\\ffmpeg\\ffprobe.exe",
        source: "managed",
      };
    case "get_settings":
      return {};
    case "lock_status":
      return { configured: !!opts?.locked, locked: !!opts?.locked };
    case "is_running":
      return scene === "dashboard";
    case "detect_encoders":
      return DETECTION;
    case "scan_inputs":
      return { count: HOME_PROJECTION_T2.candidate_files, total_bytes: HOME_PROJECTION_T2.candidate_bytes };
    case "project_reclaim":
      // Tier 2 lands shortly after, once HomeView's listener is registered —
      // exactly the two-tier flow the real backend runs.
      setTimeout(() => void emit("sqz-projection", HOME_PROJECTION_T2), 150);
      return HOME_PROJECTION_T1;
    case "get_history": {
      // Filter server-side like the real backend, so the paging total and the
      // rows shown match the active filter (not a fixed sample).
      const f = args?.filter ?? {};
      let rows = HISTORY.rows as Row[];
      if (Array.isArray(f.statuses) && f.statuses.length > 0) {
        rows = rows.filter((r) => f.statuses.includes(r.status));
      }
      if (f.search) {
        const q = String(f.search).toLowerCase();
        rows = rows.filter((r) => r.path.toLowerCase().includes(q));
      }
      return { ...HISTORY, rows };
    }
    case "plugin:dialog|open":
      return HOME_INPUTS;
    default:
      // save_settings, open_path, reveal_path, event plugin, etc.
      return null;
  }
}

type Emit = (event: string, payload: unknown) => Promise<unknown>;

/** A single live encode: file-start followed by a run of progress ticks. */
async function liveEncode(
  emit: Emit,
  path: string,
  name: string,
  duration: number,
  srcSize: number,
  ratios: number[],
  fracs: number[],
  speed: number | null,
  fps: number | null,
) {
  await emit("sqz-file-start", { path, name, duration, src_size: srcSize });
  for (let i = 0; i < fracs.length; i++) {
    const frac = fracs[i];
    const ratio = ratios[i];
    const outBytes = ratio == null ? null : Math.round(srcSize * ratio * frac);
    await emit("sqz-file-progress", {
      path,
      sec: Math.round(duration * frac),
      out_bytes: outBytes,
      fps,
      speed,
      bitrate_kbps: outBytes ? Math.round((outBytes * 8) / 1000 / (duration * frac)) : null,
    });
  }
}

async function record(
  emit: Emit,
  path: string,
  outcome: string,
  origSize: number | null,
  outSize: number | null,
  savedBytes: number,
  message = "",
) {
  await emit("sqz-file-record", {
    path,
    outcome,
    saved_bytes: savedBytes,
    message,
    orig_size: origSize,
    out_size: outSize,
  });
}

/** Scripted scenes, keyed by ?scene=. Only the dashboard needs live events. */
export const scenes: Record<string, (emit: Emit) => Promise<void>> = {
  home: async () => {},
  history: async () => {},
  dashboard: async (emit) => {
    await emit("sqz-run-start", { total: 142 });

    // A nearly-finished run: emit a bulk of already-processed files so the meter,
    // stat row, and queue read as 135 / 142 processed with 3 still in flight.
    // Counts here (115+5+7+2) plus the six curated records below sum to 135:
    // done 118, normalized 6, skipped 8, failed 3.
    let k = 0;
    const bulkPath = (i: number) => {
      const show = SHOWS[i % SHOWS.length];
      const season = 1 + Math.floor(i / 24);
      const ep = 1 + (i % 24);
      const h = GEN_RES[i % GEN_RES.length];
      const s = String(season).padStart(2, "0");
      const e = String(ep).padStart(2, "0");
      return `D:\\Media\\TV\\${show}\\S${s}E${e} ${h}p.mkv`;
    };
    for (let i = 0; i < 115; i++) {
      const orig = Math.round((3 + (i % 9) * 0.6) * GB);
      const out = Math.round(orig * (0.4 + (i % 5) * 0.03));
      await record(emit, bulkPath(k++), "done", orig, out, orig - out, "AV1 · NVENC · verified");
    }
    for (let i = 0; i < 5; i++) {
      const orig = Math.round((1.1 + (i % 3) * 0.2) * GB);
      const out = Math.round(orig * 0.98);
      await record(emit, bulkPath(k++), "normalized", orig, out, orig - out, "Remuxed to MKV");
    }
    for (let i = 0; i < 7; i++) await record(emit, bulkPath(k++), "skipped_efficient", null, null, 0, "Already efficient");
    for (let i = 0; i < 2; i++) await record(emit, bulkPath(k++), "failed", null, null, 0, "ffprobe: moov atom not found — file is truncated or still being written");

    // Curated recent files — emitted last, so they head the event log.
    await record(emit, "D:\\Media\\Movies\\Blade Runner 2049 (2017) 2160p HDR.mkv", "done", Math.round(38.4 * GB), Math.round(15.1 * GB), Math.round(23.3 * GB), "AV1 · NVENC · verified");
    await record(emit, "D:\\Media\\Movies\\Dune Part Two (2024) 2160p.mkv", "done", Math.round(41.2 * GB), Math.round(17.6 * GB), Math.round(23.6 * GB), "AV1 · NVENC · verified");
    await record(emit, "D:\\Media\\Movies\\Sintel (2010) 1080p.mkv", "done", Math.round(1.12 * GB), Math.round(438 * MB), Math.round(0.69 * GB), "AV1 · NVENC · verified");
    await record(emit, "D:\\Media\\Home\\Wedding 2011 (camcorder).avi", "normalized", Math.round(1.24 * GB), Math.round(1.19 * GB), Math.round(0.05 * GB), "Remuxed to MKV");
    await record(emit, "D:\\Media\\TV\\Arcane\\S01E03 1080p HEVC.mkv", "skipped_efficient", null, null, 0, "Already HEVC at 1080p");
    await record(emit, "D:\\Media\\Recordings\\Capture 2023-11-02.mkv", "failed", null, null, 0, "ffprobe: moov atom not found — file is truncated or still being written");

    // In-flight encodes at different states.
    await liveEncode(
      emit,
      "D:\\Media\\Movies\\Interstellar (2014) 2160p HDR.mkv",
      "Interstellar (2014) 2160p HDR.mkv",
      10140,
      Math.round(24 * GB),
      [0.47, 0.45, 0.44, 0.43, 0.42, 0.415],
      [0.1, 0.2, 0.35, 0.48, 0.56, 0.63],
      1.42,
      84,
    );
    await liveEncode(
      emit,
      "D:\\Media\\TV\\The Expanse\\S04E06 - Displacement 1080p.mkv",
      "S04E06 - Displacement 1080p.mkv",
      3300,
      Math.round(4.6 * GB),
      [0.9, 0.88, 0.87, 0.865, 0.86],
      [0.08, 0.16, 0.26, 0.34, 0.4],
      2.1,
      121,
    );
    await liveEncode(
      emit,
      "D:\\Media\\TV\\Planet Earth II\\S01E01 - Islands 2160p.mkv",
      "S01E01 - Islands 2160p.mkv",
      3180,
      Math.round(19.5 * GB),
      [null as unknown as number],
      [0.04],
      0.9,
      54,
    );
  },
};
