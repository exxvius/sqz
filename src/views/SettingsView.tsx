import { useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { FfmpegSetup } from "../components/FfmpegSetup";
import { api } from "../lib/api";
import type { Theme } from "../lib/theme";
import type { FfStatus } from "../lib/types";

interface Props {
  theme: Theme;
  toggleTheme: () => void;
  ff: FfStatus | null;
  refreshFf: () => void;
}

export function SettingsView({ theme, toggleTheme, ff, refreshFf }: Props) {
  const [cleared, setCleared] = useState(false);

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
