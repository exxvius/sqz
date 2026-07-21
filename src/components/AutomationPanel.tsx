import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { relativeTime } from "../lib/format";
import { useLock } from "../lib/lock";
import { useStore } from "../lib/store";
import { PlayIcon, WatchIcon } from "./icons";
import type { AutomationStatus } from "../lib/types";

/** A future-relative "in 3h" / "due now" label for the next scheduled run. */
function whenNext(next: number | null): string {
  if (next == null) return "—";
  const diff = next - Date.now() / 1000;
  if (diff <= 60) return "due now";
  if (diff < 3600) return `in ${Math.round(diff / 60)}m`;
  if (diff < 86400) return `in ${Math.round(diff / 3600)}h`;
  return `in ${Math.round(diff / 86400)}d`;
}

/** The schedule line for one watched library, accounting for on-change and the
 *  idle gate (a due-but-idle-gated library is waiting for you to step away). */
function scheduleLabel(
  entry: AutomationStatus["entries"][number],
  enabled: boolean,
  systemIdle: boolean,
): string {
  if (!enabled) return "paused (automation off)";
  if (entry.trigger_kind === "onchange") {
    return entry.idle_only && !systemIdle
      ? "on file change (when away)"
      : "watching for changes";
  }
  const due =
    entry.next_run_at != null && entry.next_run_at - Date.now() / 1000 <= 60;
  if (due && entry.idle_only && !systemIdle)
    return "due — waiting until you're away";
  return `next ${whenNext(entry.next_run_at)}`;
}

/**
 * The Dashboard's automation surface: the global master switch plus every watched
 * library's next/last unattended run, with a per-library "Run now". Watched
 * libraries and their schedules are configured in the Library tab; this panel is
 * the "what's sqz doing while I'm away" view.
 */
export function AutomationPanel() {
  const store = useStore();
  const { locked } = useLock();
  const [status, setStatus] = useState<AutomationStatus | null>(null);

  const refresh = useCallback(() => {
    api.getAutomation().then(setStatus);
  }, []);

  // Refresh on mount, on a slow poll (next-run countdowns), and whenever a run
  // starts/ends (an unattended run just fired or finished updates last-run).
  useEffect(refresh, [refresh]);
  useEffect(() => {
    const id = window.setInterval(refresh, 5000);
    return () => window.clearInterval(id);
  }, [refresh]);
  useEffect(refresh, [refresh, store.running]);

  if (!status) return null;
  // Keep the Live tab clean for people who don't use automation.
  if (!status.enabled && status.entries.length === 0) return null;

  const toggleMaster = async () => {
    await api.setAutomationEnabled(!status.enabled);
    refresh();
  };

  const runNow = async (id: string) => {
    if (store.running) return;
    await api.runLibraryNow(id);
  };

  // Which watched library is being run unattended right now (to highlight its row
  // in place of the old separate top-of-page badge).
  const runningId =
    store.running && store.runSource?.source === "unattended"
      ? store.runSource.library_id
      : null;

  return (
    <div className="card card-flat card-glow">
      <div className="card-head">
        <div
          className="card-title"
          style={{ display: "flex", alignItems: "center", gap: 6 }}
        >
          <WatchIcon size={15} /> Automation
        </div>
        {!locked && (
          <div
            className="seg"
            role="group"
            aria-label="Automation master switch"
          >
            <button aria-pressed={!status.enabled} onClick={toggleMaster}>
              Off
            </button>
            <button aria-pressed={status.enabled} onClick={toggleMaster}>
              On
            </button>
          </div>
        )}
      </div>

      {status.entries.length === 0 ? (
        <p className="muted" style={{ margin: "var(--space-2) 0 0" }}>
          No libraries are being watched. Turn on the eye on a saved library
          (Library tab) to run it unattended.
        </p>
      ) : (
        <div className="lib-list" style={{ marginTop: "var(--space-2)" }}>
          {status.entries.map((e) => {
            const isRunning = e.library_id === runningId;
            const meta = isRunning
              ? store.paused
                ? "running now — paused, you're active"
                : "running now…"
              : `${scheduleLabel(e, status.enabled, status.system_idle)}${
                  e.last_auto_run_at
                    ? ` · last ran ${relativeTime(e.last_auto_run_at)}`
                    : ""
                }`;
            return (
              <div
                className={`lib-row${isRunning ? " running" : ""}`}
                key={e.library_id}
              >
                <div className="lib-row-main">
                  <span className="lib-name">
                    {isRunning && <WatchIcon size={13} />} {e.name}
                  </span>
                  <span className="muted lib-meta">{meta}</span>
                </div>
                {!locked && !isRunning && (
                  <div className="lib-row-actions">
                    <button
                      className="mini-btn"
                      onClick={() => runNow(e.library_id)}
                      disabled={store.running}
                      title="Run this library now"
                      aria-label="Run now"
                    >
                      <PlayIcon /> Run now
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
