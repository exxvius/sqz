import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Codec, Detection, Encoder } from "../lib/types";

interface Props {
  codec: Codec;
  encoderOverride: string | null;
  onOverride: (name: string | null) => void;
}

const familyPill = (e: Encoder) =>
  e.family === "software" ? "pill cpu" : "pill hw";

export function EncoderPanel({ codec, encoderOverride, onOverride }: Props) {
  const [detection, setDetection] = useState<Detection | null>(null);
  const [loading, setLoading] = useState(true);

  const detect = () => {
    setLoading(true);
    api
      .detectEncoders()
      .then(setDetection)
      .finally(() => setLoading(false));
  };

  useEffect(detect, []);

  const support = detection?.codecs.find((c) => c.codec === codec);
  const usable = support?.usable ?? [];

  return (
    <div className="card">
      <div className="row between">
        <div className="card-title" style={{ margin: 0 }}>
          Hardware &amp; encoders
        </div>
        <button className="btn ghost" onClick={detect} disabled={loading}>
          {loading ? "Detecting…" : "Re-detect"}
        </button>
      </div>

      {loading && !detection ? (
        <p className="muted" style={{ marginTop: "var(--space-4)" }}>
          Probing your GPU and CPU encoders…
        </p>
      ) : (
        <>
          <p className="muted" style={{ margin: "var(--space-3) 0 var(--space-4)" }}>
            {detection?.has_hardware
              ? "Hardware acceleration available — encoding will be fast."
              : "No hardware encoder validated. Falling back to CPU (slower, still works)."}
          </p>

          <div className="field">
            <label htmlFor="enc-sel">Encoder for {codec.toUpperCase()}</label>
            <select
              id="enc-sel"
              value={encoderOverride ?? "auto"}
              onChange={(e) => onOverride(e.target.value === "auto" ? null : e.target.value)}
            >
              <option value="auto">Auto (best available)</option>
              {usable.map((e) => (
                <option key={e.name} value={e.name}>
                  {e.name}
                </option>
              ))}
            </select>
          </div>

          <div className="enc-grid" style={{ marginTop: "var(--space-3)" }}>
            {usable.length === 0 ? (
              <span className="muted">No usable {codec.toUpperCase()} encoder found.</span>
            ) : (
              usable.map((e) => (
                <div className="enc-row" key={e.name}>
                  <span className="fam mono">{e.name}</span>
                  <span className={familyPill(e)}>
                    {e.family === "software" ? "CPU" : "hardware"}
                  </span>
                  {support?.selected?.name === e.name && !encoderOverride && (
                    <span className="pill">auto-selected</span>
                  )}
                </div>
              ))
            )}
          </div>
        </>
      )}
    </div>
  );
}
