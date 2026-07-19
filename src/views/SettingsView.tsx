import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { FfmpegSetup } from "../components/FfmpegSetup";
import { PasswordModal } from "../components/PasswordModal";
import { useConfirm } from "../components/ConfirmModal";
import { Select } from "../components/Select";
import { api } from "../lib/api";
import { ACCENTS, type Accent } from "../lib/accent";
import { useLock } from "../lib/lock";
import type { CloseBehavior } from "../lib/closeBehavior";
import type { Theme } from "../lib/theme";
import type { EnvInfo, FfStatus } from "../lib/types";

interface Props {
  theme: Theme;
  toggleTheme: () => void;
  accent: Accent;
  setAccent: (a: Accent) => void;
  closeBehavior: CloseBehavior;
  setCloseBehavior: (b: CloseBehavior) => void;
  ff: FfStatus | null;
  refreshFf: () => void;
}

const CLOSE_OPTIONS: { id: CloseBehavior; label: string }[] = [
  { id: "quit", label: "Quit" },
  { id: "tray", label: "Minimize to tray" },
];

export function SettingsView({
  theme,
  accent,
  setAccent,
  closeBehavior,
  setCloseBehavior,
  ff,
  refreshFf,
}: Props) {
  const lock = useLock();
  const { confirm, element: confirmModal } = useConfirm();
  const [cleared, setCleared] = useState(false);
  const [env, setEnv] = useState<EnvInfo | null>(null);
  const [envLoading, setEnvLoading] = useState(false);
  const [configMsg, setConfigMsg] = useState<string | null>(null);
  const [changePw, setChangePw] = useState(false);

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
          <label>
            Accent color
            <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
              Recolors backgrounds, buttons, and progress. Light/Dark is in the
              sidebar. Currently {theme}.
            </div>
          </label>
          <Select
            value={accent}
            ariaLabel="Accent color"
            disabled={lock.locked}
            onChange={(v) => setAccent(v as Accent)}
            options={ACCENTS.map((a) => ({
              value: a.id,
              label: (
                <span className="accent-option">
                  <span className="accent-dot" style={{ background: a.swatch }} />
                  {a.label}
                </span>
              ),
            }))}
          />
        </div>
      </div>

      <div className="card">
        <div className="card-title">When closing the window</div>
        <p className="muted" style={{ margin: "0 0 var(--space-3)" }}>
          Choose what the window's close button does. Minimizing keeps sqz running in the
          system tray so encodes continue in the background.
        </p>
        <div className="seg" role="group" aria-label="Close behavior">
          {CLOSE_OPTIONS.map((o) => (
            <button
              key={o.id}
              aria-pressed={closeBehavior === o.id}
              disabled={lock.locked}
              onClick={() => setCloseBehavior(o.id)}
            >
              {o.label}
            </button>
          ))}
        </div>
      </div>

      <div className="card">
        <div className="card-title">Lock</div>
        <p className="muted" style={{ margin: "0 0 var(--space-3)" }}>
          Locking hides file names and paths across the app and makes it read-only —
          no run control, settings changes, or edits — useful when the machine is left
          encoding unattended. Toggle it from the lock button in the sidebar. It's
          currently{" "}
          <strong>{lock.locked ? "locked" : lock.configured ? "unlocked" : "not set up"}</strong>.
        </p>
        <button
          className="btn"
          onClick={() => setChangePw(true)}
          disabled={!lock.configured || lock.locked}
          title={
            lock.locked
              ? "Unlock the app before changing the password"
              : !lock.configured
                ? "Set up a lock password from the sidebar first"
                : undefined
          }
        >
          Change password
        </button>
        {lock.locked && (
          <p className="muted" style={{ marginTop: "var(--space-2)" }}>
            Editing and the password are locked until you unlock the app.
          </p>
        )}
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
          disabled={lock.locked}
          title={lock.locked ? "Disabled while the app is locked" : undefined}
          onClick={async () => {
            const ok = await confirm({
              title: "Clear history",
              message:
                "Clear the entire history database? Every recorded file will be forgotten. This can't be undone.",
              confirmLabel: "Clear all",
              danger: true,
            });
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
          <button className="btn ghost" onClick={loadEnv} disabled={envLoading || lock.locked}>
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
            <dd>
              {env.ffmpeg_version ??
                (env.ffmpeg_present
                  ? lock.locked
                    ? "•••••••••"
                    : env.ffmpeg_path
                  : "not set up")}
            </dd>
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
          <button className="btn" onClick={exportConfig} disabled={lock.locked}>
            Export settings
          </button>
          <button className="btn" onClick={importConfig} disabled={lock.locked}>
            Import settings
          </button>
        </div>
        {lock.locked && (
          <p className="muted" style={{ marginTop: "var(--space-2)" }}>
            Import and export are disabled while the app is locked.
          </p>
        )}
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

      {changePw && (
        <PasswordModal
          mode="change"
          onSubmit={async (v) => {
            await lock.changePassword(v.oldPassword ?? "", v.newPassword ?? "");
            setChangePw(false);
          }}
          onClose={() => setChangePw(false)}
        />
      )}
      {confirmModal}
    </div>
  );
}
