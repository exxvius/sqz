import { useState } from "react";
import { AddFolderIcon, RemoveXIcon } from "./icons";
import { pickInputs } from "../lib/api";
import { defaultWatch, MIN_INTERVAL_MINS } from "../lib/types";
import type { RunConfig, SavedLibrary, WatchConfig } from "../lib/types";
import { AdvancedOptions } from "./AdvancedOptions";
import { Collapsible } from "./Collapsible";
import { EncoderPanel } from "./EncoderPanel";
import { QualityPresets } from "./QualityPresets";

const pad = (n: number) => String(n).padStart(2, "0");

interface Props {
  /** The library being edited (a fresh one for "new"). */
  initial: SavedLibrary;
  /** Persist the edited library. Resolves when saved. */
  onSave: (lib: SavedLibrary) => Promise<void>;
  onClose: () => void;
}

/**
 * Modal for a saved library's name, folders, and full encode profile — the same
 * quality, encoder, and advanced controls as the Home run panel, so a library can
 * target anything Home can.
 */
export function LibraryEditor({ initial, onSave, onClose }: Props) {
  const [name, setName] = useState(initial.name);
  const [roots, setRoots] = useState<string[]>(initial.roots);
  const [profile, setProfile] = useState<RunConfig>(initial.profile);
  const [watch, setWatch] = useState<WatchConfig>(
    initial.watch ?? defaultWatch(),
  );
  const [saving, setSaving] = useState(false);

  const patch = (p: Partial<RunConfig>) => setProfile((c) => ({ ...c, ...p }));
  const patchWatch = (p: Partial<WatchConfig>) =>
    setWatch((w) => ({ ...w, ...p }));

  const trigger = watch.trigger;
  const setKind = (kind: "daily" | "interval") => {
    if (kind === trigger.kind) return;
    patchWatch({
      trigger:
        kind === "daily"
          ? { kind: "daily", hour: 3, minute: 0 }
          : { kind: "interval", every_mins: 60 },
    });
  };
  const onTime = (v: string) => {
    const [h, m] = v.split(":").map(Number);
    if (Number.isFinite(h) && Number.isFinite(m))
      patchWatch({ trigger: { kind: "daily", hour: h, minute: m } });
  };

  const addFolders = async () => {
    const picked = await pickInputs(true);
    if (picked.length === 0) return;
    setRoots((prev) => [...new Set([...prev, ...picked])]);
  };

  const canSave = name.trim().length > 0 && roots.length > 0 && !saving;

  const save = async () => {
    if (!canSave) return;
    setSaving(true);
    try {
      await onSave({ ...initial, name: name.trim(), roots, profile, watch });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="lib-editor-title"
      onClick={onClose}
    >
      <div className="card lib-editor" onClick={(e) => e.stopPropagation()}>
        <h2 id="lib-editor-title">
          {initial.id ? "Edit library" : "New library"}
        </h2>

        <div className="field">
          <label htmlFor="lib-name">Name</label>
          <input
            id="lib-name"
            className="search"
            placeholder="Movies, Phone clips, …"
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
          />
        </div>

        <div className="field field-stack">
          <div className="row between" style={{ width: "100%" }}>
            <label>Folders</label>
            <button className="mini-btn" onClick={addFolders}>
              <AddFolderIcon /> Add folder
            </button>
          </div>
          {roots.length > 0 ? (
            <div className="queue">
              {roots.map((r) => (
                <div className="queue-row" key={r}>
                  <span className="path" title={r}>
                    {r}
                  </span>
                  <button
                    className="rm"
                    onClick={() =>
                      setRoots((prev) => prev.filter((x) => x !== r))
                    }
                    aria-label="Remove folder"
                  >
                    <RemoveXIcon />
                  </button>
                </div>
              ))}
            </div>
          ) : (
            <p className="muted" style={{ margin: 0 }}>
              Add at least one folder for this library.
            </p>
          )}
        </div>

        <QualityPresets
          codec={profile.codec}
          quality={profile.quality}
          vmafTarget={profile.vmaf_target ?? null}
          vmafSamples={profile.vmaf_samples}
          vmafSampleSecs={profile.vmaf_sample_secs}
          onCodec={(codec) => patch({ codec, encoder_override: null })}
          onQuality={(quality) => patch({ quality })}
          onVmafTarget={(vmaf_target) => patch({ vmaf_target })}
          onVmafSamples={(vmaf_samples) => patch({ vmaf_samples })}
          onVmafSampleSecs={(vmaf_sample_secs) => patch({ vmaf_sample_secs })}
        />

        <EncoderPanel
          codec={profile.codec}
          encoderOverride={profile.encoder_override ?? null}
          onOverride={(encoder_override) => patch({ encoder_override })}
          onCodec={(codec) => patch({ codec, encoder_override: null })}
        />

        <AdvancedOptions config={profile} patch={patch} />

        <div className="card">
          <Collapsible title="Unattended (watch)">
            <div className="adv-group">
              <div className="field">
                <label>
                  Watch this library
                  <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                    Run automatically on the schedule below. The eye button on
                    the library row toggles this too.
                  </div>
                </label>
                <div className="seg" role="group" aria-label="Watch">
                  <button
                    aria-pressed={!watch.enabled}
                    onClick={() => patchWatch({ enabled: false })}
                  >
                    Off
                  </button>
                  <button
                    aria-pressed={watch.enabled}
                    onClick={() => patchWatch({ enabled: true })}
                  >
                    On
                  </button>
                </div>
              </div>

              <div className="field">
                <label>
                  Schedule
                  <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                    Only new or changed files are re-encoded — an unchanged
                    library is a no-op.
                  </div>
                </label>
                <div className="seg" role="group" aria-label="Schedule type">
                  <button
                    aria-pressed={trigger.kind === "daily"}
                    onClick={() => setKind("daily")}
                  >
                    Daily
                  </button>
                  <button
                    aria-pressed={trigger.kind === "interval"}
                    onClick={() => setKind("interval")}
                  >
                    Interval
                  </button>
                </div>
              </div>

              {trigger.kind === "daily" ? (
                <div className="field">
                  <label>
                    Time of day
                    <div
                      className="muted"
                      style={{ fontSize: "var(--text-xs)" }}
                    >
                      Local time, once per day.
                    </div>
                  </label>
                  <input
                    type="time"
                    className="search"
                    style={{ maxWidth: 140 }}
                    value={`${pad(trigger.hour)}:${pad(trigger.minute)}`}
                    onChange={(e) => onTime(e.target.value)}
                  />
                </div>
              ) : (
                <div className="field">
                  <label>
                    Every (minutes)
                    <div
                      className="muted"
                      style={{ fontSize: "var(--text-xs)" }}
                    >
                      Minimum {MIN_INTERVAL_MINS} minutes.
                    </div>
                  </label>
                  <input
                    type="number"
                    min={MIN_INTERVAL_MINS}
                    className="search"
                    style={{ maxWidth: 140 }}
                    value={trigger.every_mins}
                    onChange={(e) =>
                      patchWatch({
                        trigger: {
                          kind: "interval",
                          every_mins: Number(e.target.value) || 0,
                        },
                      })
                    }
                    onBlur={() =>
                      patchWatch({
                        trigger: {
                          kind: "interval",
                          every_mins: Math.max(
                            MIN_INTERVAL_MINS,
                            trigger.kind === "interval"
                              ? trigger.every_mins
                              : 60,
                          ),
                        },
                      })
                    }
                  />
                </div>
              )}

              <div className="field">
                <label>
                  Only when I'm away
                  <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                    Pauses automatically while you're using the machine, resumes
                    when idle.
                  </div>
                </label>
                <div className="seg" role="group" aria-label="Idle only">
                  <button
                    aria-pressed={watch.idle_only}
                    onClick={() => patchWatch({ idle_only: true })}
                  >
                    Away only
                  </button>
                  <button
                    aria-pressed={!watch.idle_only}
                    onClick={() => patchWatch({ idle_only: false })}
                  >
                    Anytime
                  </button>
                </div>
              </div>
            </div>
          </Collapsible>
        </div>

        <div className="confirm-actions">
          <button className="btn ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" onClick={save} disabled={!canSave}>
            {saving ? "Saving…" : "Save library"}
          </button>
        </div>
      </div>
    </div>
  );
}
