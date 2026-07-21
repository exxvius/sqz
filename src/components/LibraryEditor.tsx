import { useState } from "react";
import { pickInputs } from "../lib/api";
import type { RunConfig, SavedLibrary } from "../lib/types";
import { AdvancedOptions } from "./AdvancedOptions";
import { EncoderPanel } from "./EncoderPanel";
import { QualityPresets } from "./QualityPresets";

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
  const [saving, setSaving] = useState(false);

  const patch = (p: Partial<RunConfig>) => setProfile((c) => ({ ...c, ...p }));

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
      await onSave({ ...initial, name: name.trim(), roots, profile });
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
        <h2 id="lib-editor-title">{initial.id ? "Edit library" : "New library"}</h2>

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
          <label>Folders</label>
          {roots.length > 0 ? (
            <div className="queue">
              {roots.map((r) => (
                <div className="queue-row" key={r}>
                  <span className="path" title={r}>
                    {r}
                  </span>
                  <button
                    className="rm"
                    onClick={() => setRoots((prev) => prev.filter((x) => x !== r))}
                    aria-label="Remove folder"
                  >
                    ✕
                  </button>
                </div>
              ))}
            </div>
          ) : (
            <p className="muted" style={{ margin: 0 }}>
              Add at least one folder for this library.
            </p>
          )}
          <button className="mini-btn" onClick={addFolders}>
            + Add folder
          </button>
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
