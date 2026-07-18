// Presentation helpers.

export function humanBytes(n: number | null | undefined): string {
  if (!n || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let size = n;
  let u = 0;
  while (size >= 1024 && u < units.length - 1) {
    size /= 1024;
    u += 1;
  }
  return u === 0 ? `${Math.round(size)} ${units[u]}` : `${size.toFixed(1)} ${units[u]}`;
}

export function pct(frac: number): string {
  return `${Math.round(frac * 100)}%`;
}

export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/**
 * The path of the file that actually exists on disk now. A re-encoded/normalized
 * file was rewritten to `.mkv` (its original extension went to the trash), so
 * point "open" actions at the `.mkv`; anything else keeps its original path.
 */
export function currentPath(path: string, encoded: boolean): string {
  if (!encoded) return path;
  return path.replace(/\.[^./\\]+$/, ".mkv");
}

export function fmtDuration(sec: number): string {
  if (!isFinite(sec) || sec < 0) return "—";
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.floor(sec % 60);
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

/** Human duration for stats, e.g. "3h 12m", "45m 8s", "12s", "—". */
export function fmtDurationLong(sec: number | null | undefined): string {
  if (!sec || !isFinite(sec) || sec <= 0) return "—";
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.floor(sec % 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

/** Bytes-per-hour throughput as a human string, e.g. "4.2 GB/hr". */
export function fmtRate(bytes: number, seconds: number): string {
  if (!seconds || seconds <= 0 || bytes <= 0) return "—";
  return `${humanBytes((bytes / seconds) * 3600)}/hr`;
}

export function relativeTime(unixSecs: number | null): string {
  if (!unixSecs) return "";
  const diff = Date.now() / 1000 - unixSecs;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}
