// Canned IPC responses + scripted engine-event scenes for screenshotting the
// real UI in a headless browser (no Tauri backend). Shapes mirror src/lib/types.

const GB = 1024 ** 3;
const MB = 1024 ** 2;
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

// Files returned by the mocked "Add files" native picker on the Home scene.
const HOME_INPUTS = [
  "D:\\Media\\Movies\\Interstellar (2014) 2160p HDR.mkv",
  "D:\\Media\\Movies\\Dune Part Two (2024) 2160p.mkv",
  "D:\\Media\\TV\\The Expanse\\S04E06 - Displacement 1080p.mkv",
  "D:\\Media\\TV\\Planet Earth II\\S01E01 - Islands 2160p.mkv",
];

/** Handle a mocked `invoke` for a given scene. */
export function commandHandler(cmd: string, args: any, scene: string): unknown {
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
    case "is_running":
      return scene === "dashboard";
    case "detect_encoders":
      return DETECTION;
    case "scan_inputs":
      return { count: 128, total_bytes: Math.round(812 * GB) };
    case "get_history":
      return HISTORY;
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

    // Already-processed files this run → fills the meter, stat row, event log.
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
