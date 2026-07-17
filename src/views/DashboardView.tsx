import { EventLog } from "../components/EventLog";
import { LiveFiles } from "../components/LiveFiles";
import { humanBytes, pct } from "../lib/format";
import { useStore } from "../lib/store";

export function DashboardView() {
  const store = useStore();
  const active = Object.values(store.active);
  const { session, summary } = store;

  return (
    <div className="view">
      <div className="view-head">
        <h2>Live progress</h2>
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
            <div className="bar tall">
              <span
                style={{
                  width: pct(
                    store.queueTotal > 0
                      ? Math.min(session.processed / store.queueTotal, 1)
                      : 0,
                  ),
                }}
              />
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
              <span className="muted">Progress is saved — stopping is always safe.</span>
            </>
          ) : (
            <span className="muted">No active run.</span>
          )}
        </div>
      </div>

      <div className="card">
        <div className="card-title">Active encodes</div>
        <LiveFiles active={active} minSavings={store.minSavings} onAbort={store.abortFile} />
      </div>

      <div className="card">
        <div className="card-title">Event log</div>
        <EventLog log={store.log} onRetry={store.retryFile} onForce={store.forceFile} />
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
