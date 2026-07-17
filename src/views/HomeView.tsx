import { useEffect, useState } from "react";
import { DropZone } from "../components/DropZone";
import { EncoderPanel } from "../components/EncoderPanel";
import { QualityPresets } from "../components/QualityPresets";
import { NumberField, Switch } from "../components/atoms";
import { api } from "../lib/api";
import { humanBytes } from "../lib/format";
import { useStore } from "../lib/store";
import type { OnSuccess, RunConfig, ScanResult } from "../lib/types";

interface Props {
  config: RunConfig;
  setConfig: (c: RunConfig) => void;
  goDashboard: () => void;
}

const DISPOSAL: { id: OnSuccess; label: string }[] = [
  { id: "recycle", label: "Recycle Bin" },
  { id: "holding", label: "Holding folder" },
  { id: "delete", label: "Delete" },
];

export function HomeView({ config, setConfig, goDashboard }: Props) {
  const store = useStore();
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [scanning, setScanning] = useState(false);

  const patch = (p: Partial<RunConfig>) => setConfig({ ...config, ...p });

  const addInputs = (paths: string[]) => {
    const set = new Set([...config.inputs, ...paths]);
    patch({ inputs: [...set] });
  };
  const removeInput = (path: string) =>
    patch({ inputs: config.inputs.filter((p) => p !== path) });

  // Re-scan whenever the input set changes.
  useEffect(() => {
    if (config.inputs.length === 0) {
      setScan(null);
      return;
    }
    setScanning(true);
    api
      .scanInputs(config.inputs)
      .then(setScan)
      .finally(() => setScanning(false));
  }, [config.inputs]);

  const start = async () => {
    await store.start(config);
    goDashboard();
  };

  const canStart = (scan?.count ?? 0) > 0 && !store.running;

  return (
    <div className="view">
      <div className="view-head">
        <h2>Squeeze your library</h2>
        <p>
          Add videos or whole folders. sqz re-encodes each one and only replaces the original after
          verifying the result is playable, complete, and smaller.
        </p>
      </div>

      <DropZone onAdd={addInputs} />

      {config.inputs.length > 0 && (
        <div className="card" style={{ marginTop: "var(--space-4)" }}>
          <div className="row between">
            <div className="card-title" style={{ margin: 0 }}>
              {config.inputs.length} source{config.inputs.length > 1 ? "s" : ""}
            </div>
            <button className="btn ghost" onClick={() => patch({ inputs: [] })}>
              Clear
            </button>
          </div>
          <div className="queue" style={{ marginTop: "var(--space-3)" }}>
            {config.inputs.map((p) => (
              <div className="queue-row" key={p}>
                <span className="path" title={p}>
                  {p}
                </span>
                <button className="rm" onClick={() => removeInput(p)} aria-label="Remove">
                  ✕
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      <QualityPresets
        codec={config.codec}
        quality={config.quality}
        onCodec={(codec) => patch({ codec, encoder_override: null })}
        onQuality={(quality) => patch({ quality })}
      />

      <EncoderPanel
        codec={config.codec}
        encoderOverride={config.encoder_override ?? null}
        onOverride={(name) => patch({ encoder_override: name })}
      />

      <div className="card">
        <div className="card-title">When a file is replaced, the original goes to…</div>
        <div className="seg" role="group" aria-label="Disposal of originals">
          {DISPOSAL.map((d) => (
            <button
              key={d.id}
              aria-pressed={config.on_success === d.id}
              onClick={() => patch({ on_success: d.id })}
            >
              {d.label}
            </button>
          ))}
        </div>
        {config.on_success === "delete" && (
          <p className="muted" style={{ marginTop: "var(--space-3)" }}>
            Originals are permanently deleted — but only after a smaller, verified replacement is in
            place. Recycle Bin is the safest choice.
          </p>
        )}

        <details className="drawer" style={{ marginTop: "var(--space-4)" }}>
          <summary>Advanced settings</summary>
          <NumberField
            label="Parallel encodes"
            value={config.workers}
            min={1}
            max={8}
            onChange={(workers) => patch({ workers })}
          />
          <NumberField
            label="Max height (downscale taller)"
            value={config.max_height}
            min={360}
            max={4320}
            step={120}
            onChange={(max_height) => patch({ max_height })}
          />
          <NumberField
            label="Required savings (%)"
            value={Math.round(config.min_savings * 100)}
            min={0}
            max={90}
            step={5}
            onChange={(v) => patch({ min_savings: v / 100 })}
          />
          <Switch
            label="Skip already-lean files"
            hint="Don't spend an encode on sources unlikely to shrink."
            checked={config.skip_marginal}
            onChange={(skip_marginal) => patch({ skip_marginal })}
          />
          <Switch
            label="Paranoid verify"
            hint="Full-decode the output instead of a quick probe."
            checked={config.paranoid}
            onChange={(paranoid) => patch({ paranoid })}
          />
          <Switch
            label="Re-encode everything (force)"
            hint="Ignore prior results. Size gate still protects originals."
            checked={config.force}
            onChange={(force) => patch({ force })}
          />
          <Switch
            label="Dry run"
            hint="Report what would happen. Touch nothing."
            checked={config.dry_run}
            onChange={(dry_run) => patch({ dry_run })}
          />
        </details>
      </div>

      <div className="actionbar">
        <div>
          {scanning ? (
            <span className="muted">Scanning…</span>
          ) : scan ? (
            <span>
              <strong>{scan.count}</strong> video{scan.count === 1 ? "" : "s"} ·{" "}
              <span className="muted">{humanBytes(scan.total_bytes)}</span>
            </span>
          ) : (
            <span className="muted">Add sources to begin</span>
          )}
        </div>
        <div className="spacer" />
        <button className="btn primary lg" disabled={!canStart} onClick={start}>
          {config.dry_run ? "Preview run" : "Start squeezing"}
        </button>
      </div>
    </div>
  );
}
