// Presentation metadata for outcomes and manifest statuses.

import type { Outcome, Status } from "./types";

export type Tone = "ok" | "info" | "warn" | "bad" | "muted" | "accent" | "violet";

export interface StatusMeta {
  label: string;
  sym: string;
  tone: Tone;
}

export function outcomeMeta(o: Outcome): StatusMeta {
  switch (o) {
    case "done":
      return { label: "Re-encoded", sym: "✓", tone: "ok" };
    case "normalized":
      return { label: "Normalized", sym: "⇄", tone: "violet" };
    case "skipped_efficient":
      return { label: "Already efficient", sym: "»", tone: "muted" };
    case "skipped_marginal":
      return { label: "Lean — skipped", sym: "~", tone: "muted" };
    case "skipped_no_gain":
      return { label: "No gain — kept", sym: "=", tone: "warn" };
    case "failed":
      return { label: "Failed", sym: "✗", tone: "bad" };
    case "cancelled":
      return { label: "Cancelled", sym: "•", tone: "muted" };
    case "dry_run":
      return { label: "Would encode", sym: "·", tone: "info" };
  }
}

export function statusMeta(s: Status): StatusMeta {
  switch (s) {
    case "done":
      return { label: "Re-encoded", sym: "✓", tone: "ok" };
    case "normalized":
      return { label: "Normalized", sym: "⇄", tone: "violet" };
    case "skipped_already_efficient":
      return { label: "Already efficient", sym: "»", tone: "muted" };
    case "skipped_marginal":
      return { label: "Lean — skipped", sym: "~", tone: "muted" };
    case "skipped_no_gain":
      return { label: "No gain — kept", sym: "=", tone: "warn" };
    case "failed":
      return { label: "Failed", sym: "✗", tone: "bad" };
    case "processing":
      return { label: "Processing", sym: "◐", tone: "accent" };
    case "pending":
      return { label: "Pending", sym: "◇", tone: "info" };
  }
}

/** Whether a status is one a user can retry (failed) or force (skipped). */
export function retryable(s: Status): boolean {
  return s === "failed";
}
export function forceable(s: Status): boolean {
  return (
    s === "skipped_already_efficient" ||
    s === "skipped_marginal" ||
    s === "skipped_no_gain" ||
    s === "done" ||
    s === "normalized"
  );
}
