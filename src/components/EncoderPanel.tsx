import { useEffect, useState } from "react";
import { Select } from "./Select";
import { api } from "../lib/api";
import type { Codec, Detection, EncoderFamily } from "../lib/types";

interface Props {
  codec: Codec;
  encoderOverride: string | null;
  onOverride: (name: string | null) => void;
  onCodec: (c: Codec) => void;
}

const FAMILY_LABEL: Record<EncoderFamily, string> = {
  nvenc: "NVIDIA (NVENC)",
  amf: "AMD (AMF)",
  qsv: "Intel (QSV)",
  videotoolbox: "Apple (VideoToolbox)",
  software: "CPU",
};

const CODEC_LABEL: Record<Codec, string> = {
  av1: "AV1",
  hevc: "HEVC",
  h264: "H.264",
};

export function EncoderPanel({ codec, encoderOverride, onOverride, onCodec }: Props) {
  const [detection, setDetection] = useState<Detection | null>(null);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);

  const detect = () => {
    setLoading(true);
    setFailed(false);
    api
      .detectEncoders()
      .then(setDetection)
      .catch(() => setFailed(true))
      .finally(() => setLoading(false));
  };

  useEffect(detect, []);

  const support = detection?.codecs.find((c) => c.codec === codec);
  const usable = support?.usable ?? [];
  const hwForCodec = usable.find((e) => e.family !== "software");

  // GPU vendors detected across any codec (for the "your hardware" summary).
  const vendors = new Set<EncoderFamily>();
  detection?.codecs.forEach((c) =>
    c.usable.forEach((e) => e.family !== "software" && vendors.add(e.family)),
  );

  // Codecs that have a hardware encoder on this machine (for the switch tip).
  const hwCodecs = (detection?.codecs ?? [])
    .filter((c) => c.usable.some((e) => e.family !== "software") && c.codec !== codec)
    .map((c) => c.codec);

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
      ) : failed ? (
        <p className="muted" style={{ marginTop: "var(--space-4)" }}>
          Couldn't probe encoders. <button className="link-btn" onClick={detect}>Try again</button>.
        </p>
      ) : (
        <>
          <div className="hw-summary">
            <span className="muted">Detected:</span>
            {vendors.size > 0 ? (
              [...vendors].map((f) => (
                <span key={f} className="pill hw">
                  {FAMILY_LABEL[f]}
                </span>
              ))
            ) : (
              <span className="pill cpu">No GPU encoder — CPU only</span>
            )}
          </div>

          <div className="codec-matrix">
            {(detection?.codecs ?? []).map((c) => {
              const hw = c.usable.find((e) => e.family !== "software");
              return (
                <button
                  key={c.codec}
                  className={`matrix-cell${c.codec === codec ? " active" : ""}`}
                  onClick={() => onCodec(c.codec)}
                >
                  <span className="mc-codec">{CODEC_LABEL[c.codec]}</span>
                  <span className={`mc-badge ${hw ? "hw" : "cpu"}`}>
                    {hw ? FAMILY_LABEL[hw.family].replace(/ \(.*\)/, "") : "CPU"}
                  </span>
                </button>
              );
            })}
          </div>

          {hwForCodec ? (
            <p className="hw-note ok">
              Using <strong>{FAMILY_LABEL[hwForCodec.family]}</strong> hardware acceleration for{" "}
              {CODEC_LABEL[codec]}.
            </p>
          ) : (
            <p className="hw-note warn">
              Your GPU doesn't hardware-encode {CODEC_LABEL[codec]}, so it will use your CPU
              (slower, still safe).
              {hwCodecs.length > 0 && (
                <>
                  {" "}
                  For hardware speed:{" "}
                  {hwCodecs.map((c) => (
                    <button key={c} className="link-btn" onClick={() => onCodec(c)}>
                      use {CODEC_LABEL[c]}
                    </button>
                  ))}
                  .
                </>
              )}
            </p>
          )}

          <div className="field">
            <label>Encoder</label>
            <Select
              ariaLabel="Encoder"
              value={encoderOverride ?? "auto"}
              onChange={(v) => onOverride(v === "auto" ? null : v)}
              options={[
                { value: "auto", label: "Auto (best available)" },
                ...usable.map((e) => ({
                  value: e.name,
                  label: `${e.name} · ${e.family === "software" ? "CPU" : "hardware"}`,
                })),
              ]}
            />
          </div>
        </>
      )}
    </div>
  );
}
