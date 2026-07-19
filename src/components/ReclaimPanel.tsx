import { humanBytes } from "../lib/format";
import type { ReclaimProjection } from "../lib/types";

const CONFIDENCE_LABEL: Record<ReclaimProjection["confidence"], string> = {
  low: "Rough",
  fair: "Fair",
  good: "Good",
};

/** Share of the source that a reclaimable estimate represents (0..1). */
function reclaimFraction(bytes: number, est: number): number {
  if (bytes <= 0) return 0;
  return Math.min(1, est / bytes);
}

function confTitle(proj: ReclaimProjection): string {
  if (proj.cold_start) {
    return "No run history yet — a rough estimate from a static prior. It sharpens as you encode more.";
  }
  const n = proj.based_on_history_rows;
  return `Based on ${n} previously encoded file${n === 1 ? "" : "s"} in your library.`;
}

interface SummaryProps {
  proj: ReclaimProjection;
  /** Tier-2 probe pass still running (headline number will tighten). */
  refining: boolean;
  expanded: boolean;
  onToggle: () => void;
}

/** The compact readout that lives inside the floating action bar. */
export function ReclaimSummary({ proj, refining, expanded, onToggle }: SummaryProps) {
  const hasBreakdown = proj.buckets.length > 0;
  return (
    <div className="ab-readout">
      <span className="ab-count">
        <strong>{proj.candidate_files}</strong> video
        {proj.candidate_files === 1 ? "" : "s"} ·{" "}
        <span className="muted">{humanBytes(proj.candidate_bytes)}</span>
      </span>
      <span className="ab-reclaim-group" title="Estimated space this run reclaims">
        <span className="ab-reclaim">~{humanBytes(proj.est_reclaimable_bytes)}</span>
        <span className="ab-reclaim-label muted">reclaimable</span>
      </span>
      <span
        className={`reclaim-conf conf-${proj.confidence}`}
        title={confTitle(proj)}
      >
        {CONFIDENCE_LABEL[proj.confidence]}
      </span>
      {refining ? (
        <span className="reclaim-refining muted">refining…</span>
      ) : (
        hasBreakdown && (
          <button
            className={`ab-toggle${expanded ? " open" : ""}`}
            aria-expanded={expanded}
            onClick={(e) => {
              e.stopPropagation();
              onToggle();
            }}
          >
            <span className="ab-caret" aria-hidden="true">
              ›
            </span>
            {expanded ? "Hide" : "Details"}
          </button>
        )
      )}
    </div>
  );
}

/** The expanded panel: overall bar, sub-line, and the per-bucket breakdown. */
export function ReclaimBreakdown({ proj }: { proj: ReclaimProjection }) {
  const frac = reclaimFraction(proj.candidate_bytes, proj.est_reclaimable_bytes);
  return (
    <div className="reclaim-breakdown">
      <div className="reclaim-sub muted">
        <strong>~{humanBytes(proj.est_reclaimable_bytes)}</strong> reclaimable out
        of {humanBytes(proj.candidate_bytes)}
        {proj.est_skipped_files > 0 && (
          <>
            {" · "}
            <strong>{proj.est_skipped_files}</strong> skipped as already-lean
          </>
        )}
      </div>

      <div className="bar reclaim-bar" style={{ ["--p" as string]: frac }}>
        <span className="good" />
      </div>

      <div className="reclaim-table" role="table" aria-label="Reclaimable by bucket">
        <div className="reclaim-trow reclaim-thead" role="row">
          <span role="columnheader">Source</span>
          <span role="columnheader">Files</span>
          <span role="columnheader">Reclaimable</span>
          <span role="columnheader">Confidence</span>
        </div>
        {proj.buckets.map((b) => {
          // Bar length = this bucket's share of the total reclaimed space, so the
          // rows read as a proportional "where the savings come from" chart.
          const share =
            proj.est_reclaimable_bytes > 0
              ? b.est_reclaimable_bytes / proj.est_reclaimable_bytes
              : 0;
          return (
            <div className="reclaim-trow" role="row" key={`${b.src_codec}-${b.height_band}`}>
              <span role="cell" className="reclaim-src">
                <span className="reclaim-codec">{b.src_codec}</span>
                <span className="muted">{b.height_band}</span>
              </span>
              <span role="cell" className="reclaim-num">
                {b.files}
                {b.est_skipped_files > 0 && (
                  <span className="muted"> (+{b.est_skipped_files} skip)</span>
                )}
              </span>
              <span
                role="cell"
                className="reclaim-num reclaim-cell-bar"
                title={`${Math.round(share * 100)}% of the total reclaimed space`}
              >
                <span className="reclaim-amt">~{humanBytes(b.est_reclaimable_bytes)}</span>
                <span className="bar" style={{ ["--p" as string]: share }} aria-hidden="true">
                  <span className="good" />
                </span>
              </span>
              <span role="cell" className="reclaim-num muted">
                {b.sample_size > 0 ? `${Math.round(b.confidence * 100)}%` : "—"}
              </span>
            </div>
          );
        })}
      </div>

      {proj.cold_start && (
        <p className="muted reclaim-note">
          No run history yet, so this leans on a conservative estimate. It gets
          more accurate every time you encode.
        </p>
      )}
    </div>
  );
}
