import type { CSSProperties, ReactNode } from "react";
import type { ActiveFile } from "../lib/store";
import { fmtDuration, humanBytes, pct } from "../lib/format";

interface Props {
  active: ActiveFile[];
  minSavings: number;
  onAbort: (path: string) => void;
}

function projify(f: ActiveFile, minSavings: number) {
  const progress = f.duration && f.duration > 0 ? Math.min(f.sec / f.duration, 1) : 0;
  const projected = f.projections.length ? f.projections[f.projections.length - 1] : null;
  const savings = projected != null ? 1 - projected / f.srcSize : null;

  // Trend from the recent projection window: rising size = worsening.
  let trend: "up" | "down" | null = null;
  if (f.projections.length >= 4) {
    const half = Math.floor(f.projections.length / 2);
    const early = avg(f.projections.slice(0, half));
    const late = avg(f.projections.slice(half));
    const delta = (late - early) / early;
    if (Math.abs(delta) > 0.01) trend = late > early ? "up" : "down";
  }

  // ETA from realtime speed and remaining source seconds.
  let eta: number | null = null;
  if (f.speed && f.speed > 0 && f.duration) {
    eta = (f.duration - f.sec) / f.speed;
  }

  const klass =
    savings == null
      ? ""
      : savings >= minSavings
        ? "good"
        : savings > 0
          ? "warn"
          : "bad";

  return { progress, projected, savings, trend, eta, klass };
}

function avg(xs: number[]): number {
  return xs.reduce((a, b) => a + b, 0) / xs.length;
}

export function LiveFiles({ active, minSavings, onAbort }: Props) {
  if (active.length === 0) {
    return (
      <div className="empty">
        No active encodes. Add videos on the Home tab and press <strong>Start</strong>.
      </div>
    );
  }

  return (
    <div>
      {active.map((f) => {
        const d = projify(f, minSavings);
        return (
          <div className="live-card" key={f.path}>
            <div className="live-top">
              <span className="live-name" title={f.path}>
                {f.name}
              </span>
              <button className="live-abort" onClick={() => onAbort(f.path)}>
                Abort
              </button>
            </div>

            <div className="bar tall" style={{ "--p": d.progress } as CSSProperties}>
              <span className={d.klass} />
            </div>

            <div className="live-stats">
              <Stat k="progress" v={pct(d.progress)} />
              <Stat k="source" v={humanBytes(f.srcSize)} />
              <Stat
                k="now"
                v={f.outBytes != null ? humanBytes(f.outBytes) : "—"}
              />
              <Stat
                k="projected"
                v={
                  d.projected != null ? (
                    <>
                      {humanBytes(d.projected)}
                      {d.trend && (
                        <span className={`trend ${d.trend}`}>
                          {" "}
                          {d.trend === "up" ? "▲" : "▼"}
                        </span>
                      )}
                    </>
                  ) : (
                    "…"
                  )
                }
                klass={d.klass}
              />
              <Stat
                k="savings"
                v={d.savings != null ? `${(d.savings * 100).toFixed(0)}%` : "…"}
                klass={d.klass}
              />
              <Stat k="speed" v={f.speed != null ? `${f.speed.toFixed(2)}×` : "—"} />
              <Stat k="fps" v={f.fps != null ? f.fps.toFixed(0) : "—"} />
              <Stat k="eta" v={d.eta != null ? fmtDuration(d.eta) : "—"} />
            </div>
          </div>
        );
      })}
    </div>
  );
}

function Stat({
  k,
  v,
  klass,
}: {
  k: string;
  v: ReactNode;
  klass?: string;
}) {
  return (
    <div className="live-stat">
      <span className="lk">{k}</span>
      <span className={`lv ${klass ?? ""}`}>{v}</span>
    </div>
  );
}
