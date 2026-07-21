// Presentation metadata for outcomes and manifest statuses.

import type { ReactNode } from "react";
import {
  CancelledIcon,
  CheckIcon,
  DryRunIcon,
  EfficientIcon,
  FailIcon,
  LeanIcon,
  NoGainIcon,
  NormalizedIcon,
  NotScannedIcon,
  PendingIcon,
  ProcessingIcon,
} from "../components/icons";
import type { HealthState, Outcome, Status } from "./types";

export type Tone = "ok" | "info" | "warn" | "bad" | "muted" | "accent" | "violet";

export interface StatusMeta {
  label: string;
  /** The badge glyph: a text symbol or an icon element. */
  sym: ReactNode;
  tone: Tone;
}

export function outcomeMeta(o: Outcome): StatusMeta {
  switch (o) {
    case "done":
      return { label: "Re-encoded", sym: <CheckIcon />, tone: "ok" };
    case "normalized":
      return { label: "Normalized", sym: <NormalizedIcon />, tone: "violet" };
    case "skipped_efficient":
      return { label: "Already efficient", sym: <EfficientIcon />, tone: "muted" };
    case "skipped_marginal":
      return { label: "Lean — skipped", sym: <LeanIcon />, tone: "muted" };
    case "skipped_no_gain":
      return { label: "No gain — kept", sym: <NoGainIcon />, tone: "warn" };
    case "skipped_unhealthy":
      return { label: "Skipped — unhealthy", sym: <FailIcon />, tone: "warn" };
    case "original_kept":
      return { label: "Original kept", sym: <NormalizedIcon />, tone: "violet" };
    case "failed":
      return { label: "Failed", sym: <FailIcon />, tone: "bad" };
    case "cancelled":
      return { label: "Cancelled", sym: <CancelledIcon />, tone: "muted" };
    case "dry_run":
      return { label: "Would encode", sym: <DryRunIcon />, tone: "info" };
  }
}

export function statusMeta(s: Status): StatusMeta {
  switch (s) {
    case "done":
      return { label: "Re-encoded", sym: <CheckIcon />, tone: "ok" };
    case "normalized":
      return { label: "Normalized", sym: <NormalizedIcon />, tone: "violet" };
    case "skipped_already_efficient":
      return { label: "Already efficient", sym: <EfficientIcon />, tone: "muted" };
    case "skipped_marginal":
      return { label: "Lean — skipped", sym: <LeanIcon />, tone: "muted" };
    case "skipped_no_gain":
      return { label: "No gain — kept", sym: <NoGainIcon />, tone: "warn" };
    case "skipped_unhealthy":
      return { label: "Skipped — unhealthy", sym: <FailIcon />, tone: "warn" };
    case "original_kept":
      return { label: "Original kept", sym: <NormalizedIcon />, tone: "violet" };
    case "failed":
      return { label: "Failed", sym: <FailIcon />, tone: "bad" };
    case "processing":
      return { label: "Processing", sym: <ProcessingIcon />, tone: "accent" };
    case "pending":
      return { label: "Pending", sym: <PendingIcon />, tone: "info" };
    case "indexed":
      return { label: "Indexed", sym: <PendingIcon />, tone: "muted" };
  }
}

/** Presentation metadata for a library file's health verdict. */
export function healthMeta(h: HealthState | null): StatusMeta {
  switch (h) {
    case "healthy":
      return { label: "Healthy", sym: <CheckIcon />, tone: "ok" };
    case "corrupt":
      return { label: "Corrupt", sym: <FailIcon />, tone: "bad" };
    case "unreadable":
      return { label: "Unreadable", sym: <FailIcon />, tone: "bad" };
    case null:
      return { label: "Not scanned", sym: <NotScannedIcon />, tone: "muted" };
  }
}

/** Whether a status is one a user can retry (failed) or force (skipped). */
export function retryable(s: Status): boolean {
  return s === "failed";
}
export function forceable(s: Status): boolean {
  // Only files that weren't actually re-encoded — a skip you can override into a
  // real encode. A done/normalized file is already processed; forcing it again is
  // handled via History/Library actions, not this button.
  return (
    s === "skipped_already_efficient" ||
    s === "skipped_marginal" ||
    s === "skipped_no_gain"
  );
}
