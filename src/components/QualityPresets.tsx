import type { Codec, QualityPreset } from "../lib/types";

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

interface Props {
  codec: Codec;
  quality: QualityPreset;
  onCodec: (c: Codec) => void;
  onQuality: (q: QualityPreset) => void;
}

export function QualityPresets({ codec, quality, onCodec, onQuality }: Props) {
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

      <div className="presets">
        {PRESETS.map((p) => (
          <button
            key={p.id}
            className="preset"
            aria-pressed={quality === p.id}
            onClick={() => onQuality(p.id)}
          >
            <div className="p-name">{p.name}</div>
            <div className="p-desc">{p.desc}</div>
          </button>
        ))}
      </div>
    </div>
  );
}
