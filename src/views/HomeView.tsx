import { useEffect, useState } from "react";
import { Collapsible } from "../components/Collapsible";
import { DropZone } from "../components/DropZone";
import { EncoderPanel } from "../components/EncoderPanel";
import { FfmpegSetup } from "../components/FfmpegSetup";
import { QualityPresets } from "../components/QualityPresets";
import { NumberField, Switch } from "../components/atoms";
import { api } from "../lib/api";
import { humanBytes } from "../lib/format";
import { useStore } from "../lib/store";
import type { FfStatus, OnSuccess, RunConfig, ScanResult } from "../lib/types";

interface Props {
  config: RunConfig;
  setConfig: (c: RunConfig) => void;
  goDashboard: () => void;
  ff: FfStatus | null;
  refreshFf: () => void;
  goSettings: () => void;
}

const DISPOSAL: { id: OnSuccess; label: string }[] = [
  { id: "recycle", label: "Recycle Bin" },
  { id: "holding", label: "Holding folder" },
  { id: "delete", label: "Delete" },
];

export function HomeView({ config, setConfig, goDashboard, ff, refreshFf }: Props) {
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

  const ffReady = ff?.present ?? false;
  const canStart = (scan?.count ?? 0) > 0 && !store.running && ffReady;

  return (
    <div className="view">
      <div className="view-head">
        <h2>Squeeze your library</h2>
        <p>
          Add videos or whole folders. sqz re-encodes each one and only replaces the original after
          verifying the result is playable, complete, and smaller.
        </p>
      </div>

      {ff && !ff.present && (
        <div className="card notice" style={{ marginBottom: "var(--space-4)" }}>
          <div className="card-title" style={{ color: "var(--warn)" }}>
            FFmpeg required
          </div>
          <p className="muted" style={{ margin: "0 0 var(--space-2)" }}>
            sqz needs FFmpeg to encode. Download it in one click (~140&nbsp;MB, kept inside the app),
            or point sqz at your own binaries.
          </p>
          <FfmpegSetup ff={ff} onChange={refreshFf} compact />
        </div>
      )}

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
        onCodec={(codec) => patch({ codec, encoder_override: null })}
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
      </div>

      <div className="card">
        <Collapsible title="Advanced settings">
          <div className="adv-group">
            <div className="adv-group-title">Output</div>
            <NumberField
              label="Max height (downscale taller sources)"
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
              label="Custom quality"
              hint="Override the preset with a raw CQ/CRF value (lower = better, bigger)."
              checked={config.quality_override != null}
              onChange={(on) => patch({ quality_override: on ? 30 : null })}
            />
            {config.quality_override != null && (
              <NumberField
                label="Quality (CQ/CRF)"
                value={config.quality_override}
                min={0}
                max={63}
                onChange={(v) => patch({ quality_override: v })}
              />
            )}
            <Switch
              label="Normalize container to MKV"
              hint="Remux skipped/aborted files into .mkv too, so the whole library is one format."
              checked={config.normalize_container}
              onChange={(normalize_container) => patch({ normalize_container })}
            />
          </div>

          <div className="adv-group">
            <div className="adv-group-title">Speed &amp; efficiency</div>
            <NumberField
              label="Parallel encodes"
              value={config.workers}
              min={1}
              max={8}
              onChange={(workers) => patch({ workers })}
            />
            <Switch
              label="Early abort"
              hint="While encoding, project the final size and kill encodes that clearly won't pay off — stricter as they near completion."
              checked={config.early_abort}
              onChange={(early_abort) => patch({ early_abort })}
            />
            {config.early_abort && (
              <>
                <NumberField
                  label="Bloat checkpoint (%)"
                  value={Math.round(config.abort_stage1_at * 100)}
                  min={1}
                  max={20}
                  onChange={(v) => patch({ abort_stage1_at: v / 100 })}
                />
                <NumberField
                  label="Bloat margin (% over source)"
                  value={Math.round(config.abort_bloat_margin * 100)}
                  min={5}
                  max={100}
                  step={5}
                  onChange={(v) => patch({ abort_bloat_margin: v / 100 })}
                />
                <NumberField
                  label="Trend checkpoint (%)"
                  value={Math.round(config.abort_check_at * 100)}
                  min={5}
                  max={40}
                  step={5}
                  onChange={(v) => patch({ abort_check_at: v / 100 })}
                />
                <NumberField
                  label="Late-stage starts at (%)"
                  value={Math.round(config.abort_late_at * 100)}
                  min={40}
                  max={95}
                  step={5}
                  onChange={(v) => patch({ abort_late_at: v / 100 })}
                />
                <NumberField
                  label="Late-stage min savings (%)"
                  value={Math.round(config.abort_late_min_savings * 100)}
                  min={0}
                  max={20}
                  onChange={(v) => patch({ abort_late_min_savings: v / 100 })}
                />
              </>
            )}
            <Switch
              label="Skip already-lean files"
              hint="Predict up front whether a source is worth re-encoding, and skip low-payoff ones without spending an encode."
              checked={config.skip_marginal}
              onChange={(skip_marginal) => patch({ skip_marginal })}
            />
            {config.skip_marginal && (
              <NumberField
                label="Skip threshold (bits/pixel)"
                value={config.marginal_bpp}
                min={0.01}
                max={0.5}
                step={0.01}
                onChange={(marginal_bpp) => patch({ marginal_bpp })}
              />
            )}
            <Switch
              label="Hardware decode"
              hint="Decode on the GPU too (NVIDIA only). Faster, less robust on unusual sources."
              checked={config.hwaccel_decode}
              onChange={(hwaccel_decode) => patch({ hwaccel_decode })}
            />
          </div>

          <div className="adv-group">
            <div className="adv-group-title">Safety &amp; behavior</div>
            <Switch
              label="Paranoid verify"
              hint="Full-decode the output instead of a quick probe."
              checked={config.paranoid}
              onChange={(paranoid) => patch({ paranoid })}
            />
            <Switch
              label="Retry previously failed files"
              hint="Re-attempt files that errored in an earlier run."
              checked={config.retry_failed}
              onChange={(retry_failed) => patch({ retry_failed })}
            />
            <Switch
              label="Re-encode everything (force)"
              hint="Ignore prior results. The size gate still protects originals."
              checked={config.force}
              onChange={(force) => patch({ force })}
            />
            <Switch
              label="Dry run"
              hint="Report what would happen. Touch nothing."
              checked={config.dry_run}
              onChange={(dry_run) => patch({ dry_run })}
            />
          </div>
        </Collapsible>
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
