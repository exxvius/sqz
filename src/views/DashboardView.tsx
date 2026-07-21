import type { CSSProperties } from "react";
import { AutomationPanel } from "../components/AutomationPanel";
import { EventLog } from "../components/EventLog";
import { LiveFiles } from "../components/LiveFiles";
import { ClearIcon, WatchIcon } from "../components/icons";
import { humanBytes } from "../lib/format";
import { useStore } from "../lib/store";
import { useLock } from "../lib/lock";

export function DashboardView() {
  const store = useStore();
  const { locked } = useLock();
  const active = Object.values(store.active);
  const { session, summary, runSource } = store;
  const unattended =
    store.running && runSource?.source === "unattended"
      ? runSource.library_name
      : null;

  return (
    <div className="view">
      <div className="view-head">
        <h2>Live progress</h2>
        {unattended && (
          <div className="run-source-badge">
            <WatchIcon size={14} /> Unattended run of{" "}
            <strong>{unattended}</strong>
            {store.paused && " · paused — you're active"}
          </div>
        )}
        <p>
          {store.running
            ? store.paused
              ? "Paused — in-flight encodes finish, then the queue waits."
              : "Encoding. Projections turn green once a file is set to beat the size gate."
            : summary
              ? "Run complete. Originals are safe; everything is in History."
              : "Idle. Start a run from the Home tab."}
        </p>
      </div>

      <AutomationPanel />

      <div className="card">
        {store.queueTotal > 0 && (
          <div className="queue-overview">
            <div className="qo-head">
              <span className="qo-count">
                {Math.min(session.processed, store.queueTotal)}{" "}
                <span className="muted">/ {store.queueTotal} processed</span>
              </span>
              <span className="muted">
                {Math.max(0, store.queueTotal - session.processed)} remaining
                {active.length > 0 && ` · ${active.length} active`}
              </span>
            </div>
            <div
              className="bar tall"
              style={
                {
                  "--p":
                    store.queueTotal > 0
                      ? Math.min(session.processed / store.queueTotal, 1)
                      : 0,
                } as CSSProperties
              }
            >
              <span />
            </div>
          </div>
        )}

        <div className="meter">
          <span className="num">{humanBytes(session.saved)}</span>
          <span className="muted">reclaimed this run</span>
        </div>
        <div className="stat-row">
          <Stat v={session.done} k="re-encoded" />
          <Stat v={session.normalized} k="normalized" />
          <Stat v={session.skipped} k="skipped" />
          <Stat v={session.failed} k="failed" bad={session.failed > 0} />
          <Stat v={session.processed} k="processed" />
        </div>

        <div className="run-controls">
          {store.running ? (
            locked ? (
              <span className="muted">
                Run controls are locked. Encoding continues; unlock to pause or
                stop.
              </span>
            ) : (
              <>
                {store.paused ? (
                  <button className="btn primary" onClick={store.resume}>
                    Resume
                  </button>
                ) : (
                  <button className="btn" onClick={store.pause}>
                    Pause
                  </button>
                )}
                <button className="btn danger" onClick={store.cancel}>
                  Stop (resumable)
                </button>
                <span className="muted">
                  Progress is saved — stopping is always safe.
                </span>
              </>
            )
          ) : (
            <span className="muted">No active run.</span>
          )}
        </div>
      </div>

      <div className="card card-flat">
        <div className="card-title">Active encodes</div>
        <LiveFiles
          active={active}
          minSavings={store.minSavings}
          onAbort={store.abortFile}
        />
      </div>

      <div className="card card-flat">
        <div className="card-head">
          <div className="card-title">Event log</div>
          {store.log.length > 0 && (
            <button className="mini-btn" onClick={store.clearLog}>
              <ClearIcon /> Clear
            </button>
          )}
        </div>
        <EventLog
          log={store.log}
          onRetry={store.retryFile}
          onForce={store.forceFile}
        />
      </div>
    </div>
  );
}

function Stat({ v, k, bad }: { v: number; k: string; bad?: boolean }) {
  return (
    <div className="stat">
      <span className="v" style={bad ? { color: "var(--bad)" } : undefined}>
        {v}
      </span>
      <span className="k">{k}</span>
    </div>
  );
}
