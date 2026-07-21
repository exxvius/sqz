import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { AdvancedOptions } from "../components/AdvancedOptions";
import { DropZone } from "../components/DropZone";
import { EncoderPanel } from "../components/EncoderPanel";
import { FfmpegSetup } from "../components/FfmpegSetup";
import { QualityPresets } from "../components/QualityPresets";
import { ReclaimBreakdown, ReclaimSummary } from "../components/ReclaimPanel";
import { ClearIcon, RemoveXIcon } from "../components/icons";
import { api } from "../lib/api";
import { EV } from "../lib/events";
import { useStore } from "../lib/store";
import { useLock } from "../lib/lock";
import type { FfStatus, ReclaimProjection, RunConfig } from "../lib/types";

interface Props {
  config: RunConfig;
  setConfig: (c: RunConfig) => void;
  goDashboard: () => void;
  ff: FfStatus | null;
  refreshFf: () => void;
  goSettings: () => void;
}

export function HomeView({ config, setConfig, goDashboard, ff, refreshFf }: Props) {
  const store = useStore();
  const { locked, maskPath } = useLock();
  const [proj, setProj] = useState<ReclaimProjection | null>(null);
  const [scanning, setScanning] = useState(false);
  const [refining, setRefining] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const detailsRef = useRef<HTMLDivElement>(null);
  const [detailsH, setDetailsH] = useState(0);

  const patch = (p: Partial<RunConfig>) => setConfig({ ...config, ...p });

  const addInputs = (paths: string[]) => {
    const set = new Set([...config.inputs, ...paths]);
    patch({ inputs: [...set] });
  };
  const removeInput = (path: string) =>
    patch({ inputs: config.inputs.filter((p) => p !== path) });

  // Project reclaimable space whenever the input set — or any setting that
  // changes what gets skipped/estimated — changes. Tier 1 lands instantly; the
  // backend then refines it via a `sqz-projection` event. Debounced so dragging
  // a slider doesn't launch a probe storm; the previous probe pass is cancelled
  // backend-side the moment a new request arrives.
  useEffect(() => {
    if (config.inputs.length === 0) {
      setProj(null);
      setScanning(false);
      setRefining(false);
      setDetailsOpen(false);
      return;
    }
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    const timer = setTimeout(async () => {
      setScanning(true);
      setRefining(true);
      unlisten = await listen<ReclaimProjection>(EV.projection, (e) => {
        if (!cancelled) {
          setProj(e.payload);
          setRefining(false);
        }
      });
      try {
        const tier1 = await api.projectReclaim(config);
        if (!cancelled) setProj(tier1);
      } finally {
        if (!cancelled) setScanning(false);
      }
    }, 300);
    return () => {
      cancelled = true;
      clearTimeout(timer);
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    config.inputs,
    config.codec,
    config.max_height,
    config.skip_marginal,
    config.marginal_bpp,
    config.skip_dolby_vision,
    config.force,
  ]);

  const start = async () => {
    await store.start(config);
    goDashboard();
  };

  const ffReady = ff?.present ?? false;
  const hasProjection = (proj?.candidate_files ?? 0) > 0;
  const canStart = hasProjection && !store.running && ffReady;
  // The whole bar toggles the breakdown, but only once there's one to show.
  const canToggleDetails = hasProjection && !!proj && proj.buckets.length > 0 && !refining;
  const toggleDetails = () => {
    if (canToggleDetails) setDetailsOpen((o) => !o);
  };

  // The breakdown card is absolutely positioned above the sticky bar, so it
  // adds no scroll height on its own. Measure it while open and reserve that
  // much extra space at the bottom of the view, so the settings behind it can
  // always be scrolled clear of the expanded bar.
  useLayoutEffect(() => {
    const el = detailsRef.current;
    if (!detailsOpen || !el) {
      setDetailsH(0);
      return;
    }
    const measure = () => setDetailsH(el.offsetHeight);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [detailsOpen, proj]);

  return (
    <div className="view">
      <div className="view-head">
        <h2>Squeeze your library</h2>
        <p>
          {locked
            ? "The app is locked — read-only until you unlock it from the sidebar."
            : "Add videos or whole folders. sqz re-encodes each one and only replaces the original after verifying the result is playable, complete, and smaller."}
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

      {/* Everything below is disabled as a unit when the app is locked — a
          disabled fieldset natively disables every control inside it. */}
      <fieldset className="lock-fence" disabled={locked}>
      <DropZone onAdd={addInputs} disabled={locked} />

      {config.inputs.length > 0 && (
        <div className="card" style={{ marginTop: "var(--space-4)" }}>
          <div className="row between">
            <div className="card-title" style={{ margin: 0 }}>
              {config.inputs.length} source{config.inputs.length > 1 ? "s" : ""}
            </div>
            <button className="btn ghost" onClick={() => patch({ inputs: [] })}>
              <ClearIcon /> Clear
            </button>
          </div>
          <div className="queue" style={{ marginTop: "var(--space-3)" }}>
            {config.inputs.map((p) => (
              <div className="queue-row" key={p}>
                <span className="path" title={locked ? maskPath(p) : p}>
                  {maskPath(p)}
                </span>
                <button className="rm" onClick={() => removeInput(p)} aria-label="Remove">
                  <RemoveXIcon />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      <QualityPresets
        codec={config.codec}
        quality={config.quality}
        vmafTarget={config.vmaf_target ?? null}
        vmafSamples={config.vmaf_samples}
        vmafSampleSecs={config.vmaf_sample_secs}
        onCodec={(codec) => patch({ codec, encoder_override: null })}
        onQuality={(quality) => patch({ quality })}
        onVmafTarget={(vmaf_target) => patch({ vmaf_target })}
        onVmafSamples={(vmaf_samples) => patch({ vmaf_samples })}
        onVmafSampleSecs={(vmaf_sample_secs) => patch({ vmaf_sample_secs })}
      />

      <EncoderPanel
        codec={config.codec}
        encoderOverride={config.encoder_override ?? null}
        onOverride={(name) => patch({ encoder_override: name })}
        onCodec={(codec) => patch({ codec, encoder_override: null })}
      />

      <AdvancedOptions config={config} patch={patch} />
      </fieldset>

      {/* Reserve scroll room *above* the sticky dock for the open breakdown —
          its height plus the card gap it floats above the bar by — so the
          settings can be scrolled clear of it while the bar stays pinned. */}
      {detailsH > 0 && (
        <div aria-hidden="true" style={{ height: `calc(${detailsH}px + var(--space-4))` }} />
      )}

      <div className="actionbar-dock">
        {detailsOpen && hasProjection && proj && (
          <div className="actionbar-details" ref={detailsRef}>
            <ReclaimBreakdown proj={proj} />
          </div>
        )}
        <div
          className={`actionbar${canToggleDetails ? " clickable" : ""}`}
          onClick={toggleDetails}
        >
          {scanning && !proj ? (
            <span className="muted">Scanning…</span>
          ) : hasProjection && proj ? (
            <ReclaimSummary
              proj={proj}
              refining={refining}
              expanded={detailsOpen}
              onToggle={toggleDetails}
            />
          ) : (
            <span className="muted">Add sources to begin</span>
          )}
          <div className="spacer" />
          <button
            className="btn primary lg"
            disabled={!canStart || locked}
            onClick={(e) => {
              e.stopPropagation();
              start();
            }}
          >
            {config.dry_run ? "Preview run" : "Start squeezing"}
          </button>
        </div>
      </div>
    </div>
  );
}
