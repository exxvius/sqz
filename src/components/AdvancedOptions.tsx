import { Collapsible } from "./Collapsible";
import { NumberField, Switch } from "./atoms";
import { Select } from "./Select";
import { FolderIcon, RestoreIcon } from "./icons";
import { pickFolder } from "../lib/api";
import type {
  AudioMode,
  BitDepth,
  Container,
  EncoderSpeed,
  HealthGate,
  OnSuccess,
  Order,
  RunConfig,
  ScaleFilter,
  VerifyDepth,
} from "../lib/types";

// Ordered least- to most-destructive: keep the original untouched → set it aside →
// recoverable trash → gone.
const DISPOSAL: { id: OnSuccess; label: string }[] = [
  { id: "nowhere", label: "Keep both" },
  { id: "holding", label: "Holding folder" },
  { id: "recycle", label: "Recycle Bin" },
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

const HEALTH_GATE_OPTIONS: { value: HealthGate; label: string }[] = [
  { value: "off", label: "Off (encode without checking)" },
  { value: "structural", label: "Structural (probe — near-free)" },
  { value: "deep", label: "Deep (decode-probe the source)" },
];

const SCALE_OPTIONS: { value: ScaleFilter; label: string }[] = [
  { value: "lanczos", label: "Lanczos (sharpest, default)" },
  { value: "bicubic", label: "Bicubic (sharp)" },
  { value: "bilinear", label: "Bilinear (soft, no ringing)" },
  { value: "area", label: "Area (no ringing, preserves edges)" },
];

const ENCODER_SPEED_OPTIONS: { value: EncoderSpeed; label: string }[] = [
  { value: "best", label: "Best (slowest)" },
  { value: "better", label: "Better" },
  { value: "good", label: "Good" },
  { value: "balanced", label: "Balanced (default)" },
  { value: "fast", label: "Fast" },
  { value: "faster", label: "Faster" },
  { value: "fastest", label: "Fastest" },
];

const ORDER_OPTIONS: { value: Order; label: string }[] = [
  { value: "smart", label: "Smart (resume order)" },
  { value: "largest-first", label: "Largest first" },
  { value: "smallest-first", label: "Smallest first" },
  { value: "oldest-first", label: "Oldest first" },
  { value: "newest-first", label: "Newest first" },
];

// Sentinel far above any real source height: "never downscale, keep original".
const NO_CAP_HEIGHT = 20000;

const HEIGHT_OPTIONS = [
  { value: String(NO_CAP_HEIGHT), label: "No cap (keep original)" },
  { value: "4320", label: "≤ 4320p (8K)" },
  { value: "2880", label: "≤ 2880p (VR / 5.7K)" },
  { value: "2160", label: "≤ 2160p (4K UHD)" },
  { value: "1920", label: "≤ 1920p" },
  { value: "1600", label: "≤ 1600p" },
  { value: "1440", label: "≤ 1440p (QHD)" },
  { value: "1280", label: "≤ 1280p" },
  { value: "1200", label: "≤ 1200p" },
  { value: "1080", label: "≤ 1080p (FHD, default)" },
  { value: "900", label: "≤ 900p" },
  { value: "720", label: "≤ 720p (HD)" },
  { value: "576", label: "≤ 576p (PAL)" },
  { value: "480", label: "≤ 480p" },
  { value: "360", label: "≤ 360p" },
];

interface Props {
  config: RunConfig;
  patch: (p: Partial<RunConfig>) => void;
}

/**
 * The disposal choice plus the three advanced-settings sections (Output &
 * format, Speed & efficiency, Safety & behavior). Shared by the Home run panel
 * and the saved-library profile editor so the two never drift.
 */
export function AdvancedOptions({ config, patch }: Props) {
  return (
    <>
      <div className="card">
        <div className="card-title">After a successful encode, the original…</div>
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
            place. Keep both or Recycle Bin are safer choices.
          </p>
        )}
        {config.on_success === "nowhere" && (
          <p className="muted" style={{ marginTop: "var(--space-3)" }}>
            The original is left untouched and the encoded copy is written alongside it (a numbered
            name like “Movie (1).mkv” if they’d collide). Nothing is replaced — you keep both files.
          </p>
        )}
        {config.on_success === "holding" && (
          <div className="field field-stack" style={{ marginTop: "var(--space-3)" }}>
            <label>
              Holding folder
              <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                Originals are moved here (mirrored by volume). Leave as the default app folder or
                choose your own.
              </div>
            </label>
            <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "center", width: "100%" }}>
              <span className="path" style={{ flex: 1, minWidth: 0 }} title={config.holding_dir ?? undefined}>
                {config.holding_dir ?? "Default app folder"}
              </span>
              <button
                className="mini-btn"
                onClick={async () => {
                  const f = await pickFolder("Choose holding folder");
                  if (f) patch({ holding_dir: f });
                }}
              >
                <FolderIcon /> Choose…
              </button>
              {config.holding_dir && (
                <button className="mini-btn" onClick={() => patch({ holding_dir: null })}>
                  <RestoreIcon /> Reset
                </button>
              )}
            </div>
          </div>
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
            <div className="field">
              <label>
                Resolution cap
                <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                  Downscale sources taller than this; never upscale.
                </div>
              </label>
              <Select
                value={String(config.max_height)}
                options={HEIGHT_OPTIONS}
                ariaLabel="Resolution cap"
                onChange={(v) => patch({ max_height: Number(v) })}
              />
            </div>
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
                Encoder speed
                <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                  Speed vs. quality-per-size on the hardware (NVENC) encoder. Slower is
                  smaller/better; faster reclaims sooner. Balanced is a good default.
                </div>
              </label>
              <Select
                value={config.encoder_speed}
                options={ENCODER_SPEED_OPTIONS}
                ariaLabel="Encoder speed"
                onChange={(v) => patch({ encoder_speed: v as EncoderSpeed })}
              />
            </div>
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
              label="GPU-resident pipeline"
              hint="Keep decode, scaling, and encode on the GPU (NVIDIA NVENC) — no GPU↔CPU copies. Falls back to software automatically when a source isn't supported. Turn off to force software decode."
              checked={config.hardware_decode}
              onChange={(hardware_decode) => patch({ hardware_decode })}
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
            <div className="field">
              <label>
                Health-check before encoding
                <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
                  Check each source before encoding it. Unreadable or corrupt files
                  are skipped and flagged, never encoded. Deep also decode-probes the
                  source to catch silent corruption.
                </div>
              </label>
              <Select
                value={config.health_gate}
                options={HEALTH_GATE_OPTIONS}
                ariaLabel="Health-check before encoding"
                onChange={(v) => patch({ health_gate: v as HealthGate })}
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
    </>
  );
}
