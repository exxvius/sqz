import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Collapsible } from "../components/Collapsible";
import { DropZone } from "../components/DropZone";
import { EncoderPanel } from "../components/EncoderPanel";
import { FfmpegSetup } from "../components/FfmpegSetup";
import { QualityPresets } from "../components/QualityPresets";
import { ReclaimBreakdown, ReclaimSummary } from "../components/ReclaimPanel";
import { NumberField, Switch } from "../components/atoms";
import { Select } from "../components/Select";
import { api } from "../lib/api";
import { EV } from "../lib/events";
import { useStore } from "../lib/store";
import { useLock } from "../lib/lock";
import type {
  AudioMode,
  BitDepth,
  Container,
  FfStatus,
  OnSuccess,
  Order,
  ReclaimProjection,
  RunConfig,
  ScaleFilter,
  VerifyDepth,
} from "../lib/types";

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

const CONTAINERS: { id: Container; label: string }[] = [
  { id: "mkv", label: "MKV" },
  { id: "mp4", label: "MP4" },
];

const BIT_DEPTH_OPTIONS: { value: BitDepth; label: string }[] = [
  { value: "source", label: "Match source" },
  { value: "8", label: "8-bit" },
  { value: "10", label: "10-bit (HDR-ready, less banding)" },
];

const AUDIO_OPTIONS: { value: AudioMode; label: string }[] = [
  { value: "copy", label: "Copy (lossless)" },
  { value: "opus", label: "Opus" },
  { value: "aac", label: "AAC" },
];

const VERIFY_OPTIONS: { value: VerifyDepth; label: string }[] = [
  { value: "fast", label: "Fast (head + tail)" },
  { value: "thorough", label: "Thorough (full video)" },
  { value: "checksummed", label: "Checksummed (all streams)" },
];

const SCALE_OPTIONS: { value: ScaleFilter; label: string }[] = [
  { value: "lanczos", label: "Lanczos (sharpest, default)" },
  { value: "bicubic", label: "Bicubic (sharp)" },
  { value: "bilinear", label: "Bilinear (soft, no ringing)" },
  { value: "area", label: "Area (no ringing, preserves edges)" },
];

const ORDER_OPTIONS: { value: Order; label: string }[] = [
  { value: "smart", label: "Smart (resume order)" },
  { value: "largest-first", label: "Largest first" },
  { value: "smallest-first", label: "Smallest first" },
  { value: "oldest-first", label: "Oldest first" },
  { value: "newest-first", label: "Newest first" },
];

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
              Clear
            </button>
          </div>
          <div className="queue" style={{ marginTop: "var(--space-3)" }}>
            {config.inputs.map((p) => (
              <div className="queue-row" key={p}>
                <span className="path" title={locked ? maskPath(p) : p}>
                  {maskPath(p)}
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
        <Collapsible title="Output & format">
          <div className="adv-group">
            <div className="field">
              <label>
                Container
                <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                  MKV holds anything; MP4 suits stricter players/TVs.
                </div>
              </label>
              <div className="seg" role="group" aria-label="Output container">
                {CONTAINERS.map((c) => (
                  <button
                    key={c.id}
                    aria-pressed={config.container === c.id}
                    onClick={() => patch({ container: c.id })}
                  >
                    {c.label}
                  </button>
                ))}
              </div>
            </div>
            <div className="field">
              <label>
                Bit depth
                <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                  10-bit improves compression and reduces banding, even from an 8-bit source.
                  AV1 and HEVC support it; hardware H.264 falls back to 8-bit.
                </div>
              </label>
              <Select
                value={config.bit_depth}
                options={BIT_DEPTH_OPTIONS}
                ariaLabel="Output bit depth"
                onChange={(v) => patch({ bit_depth: v as BitDepth })}
              />
            </div>
            <div className="field">
              <label>
                Audio
                <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                  Copy keeps audio untouched. Opus/AAC shrink large tracks (MP4 uses AAC).
                </div>
              </label>
              <Select
                value={config.audio_mode}
                options={AUDIO_OPTIONS}
                ariaLabel="Audio handling"
                onChange={(v) => patch({ audio_mode: v as AudioMode })}
              />
            </div>
            {config.audio_mode !== "copy" && (
              <NumberField
                label="Audio bitrate (kbit/s)"
                value={config.audio_bitrate_kbps}
                min={32}
                max={512}
                step={16}
                onChange={(audio_bitrate_kbps) => patch({ audio_bitrate_kbps })}
              />
            )}
            <NumberField
              label="Max height (downscale taller sources)"
              value={config.max_height}
              min={360}
              max={4320}
              step={120}
              onChange={(max_height) => patch({ max_height })}
            />
            <div className="field">
              <label>
                Downscale filter
                <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                  Only used when a source is taller than the max height. Lanczos is
                  sharpest but rings at high-contrast edges; Area avoids ringing and
                  keeps hard edges clean.
                </div>
              </label>
              <Select
                value={config.scale_filter}
                options={SCALE_OPTIONS}
                ariaLabel="Downscale filter"
                onChange={(v) => patch({ scale_filter: v as ScaleFilter })}
              />
            </div>
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
        </Collapsible>
      </div>

      <div className="card">
        <Collapsible title="Speed & efficiency">
          <div className="adv-group">
            <Switch
              label="Auto-detect parallel encodes"
              hint="Pick a sensible worker count from this machine's CPU cores."
              checked={config.workers === 0}
              onChange={(on) => patch({ workers: on ? 0 : 2 })}
            />
            {config.workers !== 0 && (
              <NumberField
                label="Parallel encodes"
                value={config.workers}
                min={1}
                max={8}
                onChange={(workers) => patch({ workers })}
              />
            )}
            <div className="field">
              <label>
                Processing order
                <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                  Largest-first reclaims space soonest.
                </div>
              </label>
              <Select
                value={config.order}
                options={ORDER_OPTIONS}
                ariaLabel="Processing order"
                onChange={(v) => patch({ order: v as Order })}
              />
            </div>
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
        </Collapsible>
      </div>

      <div className="card">
        <Collapsible title="Safety & behavior">
          <div className="adv-group">
            <div className="field">
              <label>
                Verification depth
                <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                  How much of each output to decode before trusting it. Stricter is
                  safer but slower.
                </div>
              </label>
              <Select
                value={config.verify_depth}
                options={VERIFY_OPTIONS}
                ariaLabel="Verification depth"
                onChange={(v) => patch({ verify_depth: v as VerifyDepth, paranoid: false })}
              />
            </div>
            <Switch
              label="Perceptual quality floor (SSIM)"
              hint="Reject an encode and keep the original if it drops below a quality threshold (same-resolution outputs only)."
              checked={config.ssim_floor != null}
              onChange={(on) => patch({ ssim_floor: on ? 0.95 : null })}
            />
            {config.ssim_floor != null && (
              <NumberField
                label="Minimum SSIM (0–1)"
                value={config.ssim_floor}
                min={0.8}
                max={1}
                step={0.01}
                onChange={(ssim_floor) => patch({ ssim_floor })}
              />
            )}
            <Switch
              label="Skip Dolby Vision sources"
              hint="Re-encoding drops the Dolby Vision layer. Leave on to skip DV files (they can still be container-normalized losslessly)."
              checked={config.skip_dolby_vision}
              onChange={(skip_dolby_vision) => patch({ skip_dolby_vision })}
            />
            {config.on_success === "holding" && (
              <NumberField
                label="Holding retention (days, 0 = keep forever)"
                value={config.holding_retention_days}
                min={0}
                max={365}
                step={1}
                onChange={(holding_retention_days) => patch({ holding_retention_days })}
              />
            )}
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
