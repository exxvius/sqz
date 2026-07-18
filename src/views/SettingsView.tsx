import { useEffect, useState } from "react";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { FfmpegSetup } from "../components/FfmpegSetup";
import { api } from "../lib/api";
import type { Theme } from "../lib/theme";
import type { EnvInfo, FfStatus } from "../lib/types";

interface Props {
  theme: Theme;
  toggleTheme: () => void;
  ff: FfStatus | null;
  refreshFf: () => void;
}

export function SettingsView({ theme, toggleTheme, ff, refreshFf }: Props) {
  const [cleared, setCleared] = useState(false);
  const [env, setEnv] = useState<EnvInfo | null>(null);
  const [envLoading, setEnvLoading] = useState(false);
  const [configMsg, setConfigMsg] = useState<string | null>(null);

  const loadEnv = () => {
    setEnvLoading(true);
    api
      .environment()
      .then(setEnv)
      .catch(() => setEnv(null))
      .finally(() => setEnvLoading(false));
  };
  useEffect(loadEnv, []);

  const exportConfig = async () => {
    const dest = await save({
      defaultPath: "sqz-settings.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!dest) return;
    await api.exportSettings(dest);
    setConfigMsg("Settings exported.");
  };
  const importConfig = async () => {
    const src = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof src !== "string") return;
    try {
      await api.importSettings(src);
      // Re-bootstrap so imported settings apply everywhere immediately.
      location.reload();
    } catch (e) {
      setConfigMsg(e instanceof Error ? e.message : "Import failed.");
    }
  };

  const hw = env?.detection?.codecs
    .filter((c) => c.selected && c.selected.family !== "software")
    .map((c) => `${c.codec.toUpperCase()} · ${c.selected?.family.toUpperCase()}`);

  return (
    <div className="view">
      <div className="view-head">
        <h2>Settings</h2>
        <p>Appearance, bundled tooling, and data. Run options live under Home → Advanced and save automatically.</p>
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
        <div className="card-title">FFmpeg</div>
        <FfmpegSetup ff={ff} onChange={refreshFf} />
      </div>

      <div className="card">
        <div className="card-title">Data</div>
        <p className="muted">
          The manifest records every file sqz has read or touched. Manage individual entries in the
          History tab, or wipe the whole database here.
        </p>
        <button
          className="btn danger"
          onClick={async () => {
            const ok = await confirm(
              "Clear the entire history database? Every recorded file will be forgotten. This can't be undone.",
              { title: "Clear history", kind: "warning", okLabel: "Clear all", cancelLabel: "Cancel" },
            );
            if (!ok) return;
            await api.clearHistory();
            setCleared(true);
          }}
        >
          Clear entire history database
        </button>
        {cleared && (
          <p className="muted" style={{ marginTop: "var(--space-2)" }}>
            History cleared.
          </p>
        )}
      </div>

      <div className="card">
        <div className="row between">
          <div className="card-title" style={{ margin: 0 }}>
            Environment
          </div>
          <button className="btn ghost" onClick={loadEnv} disabled={envLoading}>
            {envLoading ? "Checking…" : "Re-check"}
          </button>
        </div>
        <p className="muted" style={{ margin: "var(--space-2) 0 var(--space-3)" }}>
          What sqz detected on this machine. Encoders are validated by a real test
          encode, not just assumed present.
        </p>
        {env ? (
          <dl className="kv-grid">
            <dt>OS</dt>
            <dd>
              {env.os} · {env.arch}
            </dd>
            <dt>CPU cores</dt>
            <dd>{env.cpus}</dd>
            <dt>Locale</dt>
            <dd>{env.locale}</dd>
            <dt>FFmpeg</dt>
            <dd>{env.ffmpeg_version ?? (env.ffmpeg_present ? env.ffmpeg_path : "not set up")}</dd>
            <dt>Hardware encoders</dt>
            <dd>{hw && hw.length > 0 ? hw.join(", ") : "none detected (CPU fallback)"}</dd>
          </dl>
        ) : (
          <p className="muted">{envLoading ? "Checking…" : "Unavailable."}</p>
        )}
      </div>

      <div className="card">
        <div className="card-title">Configuration</div>
        <p className="muted" style={{ margin: "0 0 var(--space-3)" }}>
          Save your run options to a file, or load them on another machine. Importing
          replaces the current settings and reloads the app.
        </p>
        <div className="row" style={{ gap: "var(--space-2)" }}>
          <button className="btn" onClick={exportConfig}>
            Export settings
          </button>
          <button className="btn" onClick={importConfig}>
            Import settings
          </button>
        </div>
        {configMsg && (
          <p className="muted" style={{ marginTop: "var(--space-2)" }}>
            {configMsg}
          </p>
        )}
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
