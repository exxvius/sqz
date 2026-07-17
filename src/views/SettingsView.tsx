import { useEffect, useState } from "react";
import { NumberField } from "../components/atoms";
import { api } from "../lib/api";
import type { Theme } from "../lib/theme";
import type { FfStatus } from "../lib/types";

interface Props {
  theme: Theme;
  toggleTheme: () => void;
}

export function SettingsView({ theme, toggleTheme }: Props) {
  const [ff, setFf] = useState<FfStatus | null>(null);
  const [defaults, setDefaults] = useState<Record<string, unknown>>({});

  useEffect(() => {
    api.ffmpegStatus().then(setFf);
    api.getSettings().then(setDefaults);
  }, []);

  const save = (patch: Record<string, unknown>) => {
    const next = { ...defaults, ...patch };
    setDefaults(next);
    api.saveSettings(next);
  };

  const workers = (defaults.workers as number) ?? 2;

  return (
    <div className="view">
      <div className="view-head">
        <h2>Settings</h2>
        <p>Defaults for new runs, appearance, and bundled tooling.</p>
      </div>

      <div className="card">
        <div className="card-title">Appearance</div>
        <div className="field">
          <label>Theme</label>
          <button className="theme-toggle" onClick={toggleTheme}>
            {theme === "dark" ? "🌙 Dark" : "☀️ Light"}
          </button>
        </div>
      </div>

      <div className="card">
        <div className="card-title">Run defaults</div>
        <NumberField
          label="Parallel encodes"
          value={workers}
          min={1}
          max={8}
          onChange={(v) => save({ workers: v })}
        />
        <p className="muted">
          2–3 is ideal on a single hardware encoder. More can help pure-CPU encoding on many cores.
        </p>
      </div>

      <div className="card">
        <div className="card-title">Bundled FFmpeg</div>
        <div className="enc-row">
          <span className={`status-dot ${ff?.present ? "ok" : "bad"}`} />
          <span>{ff?.present ? "Bundled and ready" : "Not found — using system PATH"}</span>
        </div>
        <div className="field">
          <label>ffmpeg</label>
          <span className="mono muted" style={{ fontSize: "var(--text-xs)" }}>
            {ff?.ffmpeg}
          </span>
        </div>
        <div className="field">
          <label>ffprobe</label>
          <span className="mono muted" style={{ fontSize: "var(--text-xs)" }}>
            {ff?.ffprobe}
          </span>
        </div>
      </div>

      <div className="card">
        <div className="card-title">About</div>
        <p className="muted">
          sqz is MIT-licensed. Released builds bundle GPL-licensed FFmpeg; the combined package is
          governed by the GPL. FFmpeg is a trademark of Fabrice Bellard; sqz is not affiliated with
          the FFmpeg project.
        </p>
      </div>
    </div>
  );
}
