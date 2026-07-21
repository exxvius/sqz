import { useEffect, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, pickBinary } from "../lib/api";
import { RestoreIcon } from "./icons";
import { humanBytes } from "../lib/format";
import { useLock } from "../lib/lock";
import type { FfStatus, FfmpegProgress } from "../lib/types";

interface Props {
  ff: FfStatus | null;
  onChange: () => void;
  compact?: boolean;
}

const SOURCE_LABEL: Record<string, string> = {
  custom: "your own binary",
  managed: "downloaded by sqz",
  system: "found on PATH",
  none: "not found",
};

export function FfmpegSetup({ ff, onChange, compact }: Props) {
  const { locked } = useLock();
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<FfmpegProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const un = listen<FfmpegProgress>("sqz-ffmpeg-progress", (e) =>
      setProgress(e.payload),
    );
    return () => {
      un.then((f) => f());
    };
  }, []);

  const download = async () => {
    setBusy(true);
    setError(null);
    setProgress(null);
    try {
      await api.downloadFfmpeg();
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const useOwn = async () => {
    setError(null);
    const ffmpeg = await pickBinary("Select your ffmpeg binary");
    if (!ffmpeg) return;
    const ffprobe = await pickBinary("Select your ffprobe binary");
    if (!ffprobe) return;
    setBusy(true);
    try {
      await api.setFfmpegPaths(ffmpeg, ffprobe);
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const reset = async () => {
    await api.clearFfmpegOverride();
    onChange();
  };

  const present = ff?.present ?? false;
  const frac =
    progress && progress.total > 0 ? progress.downloaded / progress.total : 0;

  return (
    <div>
      {!compact && (
        <div className="enc-row" style={{ marginBottom: "var(--space-3)" }}>
          <span className={`status-dot ${present ? "live" : "idle"}`} />
          <span>
            {present ? (
              <>
                FFmpeg ready ·{" "}
                <span className="muted">{SOURCE_LABEL[ff!.source]}</span>
              </>
            ) : (
              "FFmpeg is required to encode. Download it (one click) or point sqz at your own."
            )}
          </span>
        </div>
      )}

      {busy && progress ? (
        <div style={{ margin: "var(--space-3) 0" }}>
          <div className="bar tall" style={{ "--p": frac } as CSSProperties}>
            <span />
          </div>
          <div
            className="muted"
            style={{ fontSize: "var(--text-xs)", marginTop: "var(--space-2)" }}
          >
            {progress.stage === "extract"
              ? "Extracting…"
              : `Downloading ${humanBytes(progress.downloaded)}${
                  progress.total ? ` / ${humanBytes(progress.total)}` : ""
                }`}
          </div>
        </div>
      ) : null}

      <div
        className="card-actions"
        style={{ marginTop: compact ? "var(--space-2)" : 0 }}
      >
        {!present && (
          <button
            className="btn primary"
            onClick={download}
            disabled={busy || locked}
          >
            {busy ? "Downloading…" : "Download FFmpeg"}
          </button>
        )}
        {present && (
          <button className="btn" onClick={download} disabled={busy || locked}>
            Re-download
          </button>
        )}
        <button
          className="btn ghost"
          onClick={useOwn}
          disabled={busy || locked}
        >
          Use my own…
        </button>
        {ff?.source === "custom" && (
          <button
            className="btn ghost"
            onClick={reset}
            disabled={busy || locked}
          >
            <RestoreIcon /> Reset to auto
          </button>
        )}
      </div>

      {error && (
        <div className="err-box" style={{ marginTop: "var(--space-3)" }}>
          {error}
        </div>
      )}

      {!compact && present && (
        <div className="field" style={{ marginTop: "var(--space-3)" }}>
          <label>ffmpeg</label>
          <span className="mono muted" style={{ fontSize: "var(--text-xs)" }}>
            {locked ? "•••••••••" : ff?.ffmpeg}
          </span>
        </div>
      )}
    </div>
  );
}
