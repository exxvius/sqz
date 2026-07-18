import type { CSSProperties } from "react";
import type { ActiveFile } from "../lib/store";
import { humanBytes, pct } from "../lib/format";

interface Props {
  active: ActiveFile[];
  minSavings: number;
}

interface Derived {
  progress: number;
  projected: number | null;
  klass: "good" | "warn" | "bad" | "";
  label: string;
}

function derive(f: ActiveFile, minSavings: number): Derived {
  const progress = f.duration && f.duration > 0 ? Math.min(f.sec / f.duration, 1) : 0;
  if (!f.outBytes || progress <= 0) {
    return { progress, projected: null, klass: "", label: "estimating…" };
  }
  const projected = f.outBytes / progress;
  const threshold = f.srcSize * (1 - minSavings);
  // green = beats the size gate, yellow = smaller but not enough, red = growing.
  const klass: Derived["klass"] =
    projected <= threshold ? "good" : projected <= f.srcSize ? "warn" : "bad";
  const delta = ((projected - f.srcSize) / f.srcSize) * 100;
  const label = `~${humanBytes(projected)} (${delta >= 0 ? "+" : ""}${delta.toFixed(0)}%)`;
  return { progress, projected, klass, label };
}

export function WorkerBars({ active, minSavings }: Props) {
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
        const d = derive(f, minSavings);
        return (
          <div className="worker" key={f.path}>
            <div className="worker-head">
              <span className="wname" title={f.path}>
                {f.name}
              </span>
              <span className="muted mono">{humanBytes(f.srcSize)}</span>
            </div>
            <div className="bar" style={{ "--p": d.progress } as CSSProperties}>
              <span />
            </div>
            <div className="worker-meta">
              <span>{pct(d.progress)}</span>
              <span className={`proj ${d.klass}`}>{d.label}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
