import { useState, type CSSProperties } from "react";
import type { Codec, QualityPreset } from "../lib/types";
import { Switch } from "./atoms";

const PRESETS: { id: QualityPreset; name: string; desc: string }[] = [
  { id: "max-savings", name: "Maximum savings", desc: "Smallest files. Great for archives." },
  { id: "balanced", name: "Balanced", desc: "Near-transparent, strong savings. Default." },
  { id: "high-quality", name: "High quality", desc: "Bigger files, very close to source." },
  { id: "visually-lossless", name: "Visually lossless", desc: "Largest. Only for keepers." },
];

const CODECS: { id: Codec; label: string; note: string }[] = [
  { id: "av1", label: "AV1", note: "best efficiency" },
  { id: "hevc", label: "HEVC", note: "wide support" },
  { id: "h264", label: "H.264", note: "plays anywhere" },
];

/** VMAF target slider range + default when the mode is switched on. */
const VMAF_MIN = 90;
const VMAF_MAX = 99;
const VMAF_DEFAULT = 95;
/** Interior click-point notches (one per step, excluding the two endpoints). */
const TICKS = Array.from({ length: VMAF_MAX - VMAF_MIN + 1 }, (_, i) => i).slice(1, -1);
/** Magnification falloff width (fraction of the track) around the cursor. */
const TICK_SIGMA = 0.08;

interface Props {
  codec: Codec;
  quality: QualityPreset;
  vmafTarget: number | null;
  onCodec: (c: Codec) => void;
  onQuality: (q: QualityPreset) => void;
  onVmafTarget: (t: number | null) => void;
}

export function QualityPresets({
  codec,
  quality,
  vmafTarget,
  onCodec,
  onQuality,
  onVmafTarget,
}: Props) {
  const vmafOn = vmafTarget != null;
  // Cursor position over the slider (0–1), for dock-style tick magnification.
  const [hoverX, setHoverX] = useState<number | null>(null);
  const trackCursor = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    setHoverX(Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width)));
  };

  return (
    <div className="card">
      <div className="card-title">Codec &amp; quality</div>

      <div className="row between" style={{ marginBottom: "var(--space-4)" }}>
        <div className="seg" role="group" aria-label="Target codec">
          {CODECS.map((c) => (
            <button
              key={c.id}
              aria-pressed={codec === c.id}
              onClick={() => onCodec(c.id)}
              title={c.note}
            >
              {c.label}
            </button>
          ))}
        </div>
        <span className="muted">{CODECS.find((c) => c.id === codec)?.note}</span>
      </div>

      {codec === "av1" && (
        <p className="muted compat-note">
          Heads-up: AV1 can't Direct Play on many older Plex, TV, and browser clients — they may
          transcode or fail to play. Great for archives; check your playback devices first.
        </p>
      )}

      <div className={`presets${vmafOn ? " inactive" : ""}`} aria-disabled={vmafOn}>
        {PRESETS.map((p) => (
          <button
            key={p.id}
            className="preset"
            aria-pressed={!vmafOn && quality === p.id}
            disabled={vmafOn}
            onClick={() => onQuality(p.id)}
          >
            <div className="p-name">{p.name}</div>
            <div className="p-desc">{p.desc}</div>
          </button>
        ))}
      </div>

      <div className="vmaf-mode">
        <Switch
          checked={vmafOn}
          onChange={(on) => onVmafTarget(on ? VMAF_DEFAULT : null)}
          label="Target a quality (VMAF)"
          hint="Finds the smallest file per video that still hits a perceptual-quality target, instead of a fixed preset. Costs extra sample-encodes; cached per file so re-runs are fast."
        />
        {vmafOn && (
          <div className="vmaf-slider">
            <div className="vmaf-row">
              <label className="vmaf-label">
                Target VMAF
                <span className="vmaf-value">{vmafTarget}</span>
              </label>
              <div
                className="range"
                onMouseMove={trackCursor}
                onMouseLeave={() => setHoverX(null)}
                style={
                  {
                    // Position of the fill edge + knob along the track.
                    "--fill": `${
                      (((vmafTarget ?? VMAF_DEFAULT) - VMAF_MIN) / (VMAF_MAX - VMAF_MIN)) * 100
                    }%`,
                  } as CSSProperties
                }
              >
                <input
                  type="range"
                  min={VMAF_MIN}
                  max={VMAF_MAX}
                  step={1}
                  value={vmafTarget ?? VMAF_DEFAULT}
                  onChange={(e) => onVmafTarget(Number(e.target.value))}
                  aria-label="Target VMAF"
                />
                <div className="range-fill" />
                <div className="range-ticks" aria-hidden="true">
                  {TICKS.map((i) => {
                    const pos = i / (VMAF_MAX - VMAF_MIN); // 0–1
                    const mag =
                      hoverX == null
                        ? 0
                        : Math.exp(-(((pos - hoverX) / TICK_SIGMA) ** 2) / 2);
                    return (
                      <span
                        key={i}
                        style={{ left: `${pos * 100}%`, "--mag": mag.toFixed(3) } as CSSProperties}
                      />
                    );
                  })}
                </div>
                <div className="range-thumb" />
              </div>
            </div>
            <div className="muted vmaf-hint">
              95 is near-transparent. Higher targets keep more quality but reclaim less; the
              per-title search picks the CRF that hits it.
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
