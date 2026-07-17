import { EventLog } from "../components/EventLog";
import { WorkerBars } from "../components/WorkerBars";
import { humanBytes } from "../lib/format";
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
              ? "Run complete. Your originals are safe; savings are recorded in History."
              : "Idle. Start a run from the Home tab."}
        </p>
      </div>

      <div className="card">
        <div className="meter">
          <span className="num">{humanBytes(session.saved)}</span>
          <span className="muted">reclaimed this run</span>
        </div>
        <div className="stat-row">
          <div className="stat">
            <span className="v">{session.done}</span>
            <span className="k">re-encoded</span>
          </div>
          <div className="stat">
            <span className="v">{session.skipped}</span>
            <span className="k">skipped</span>
          </div>
          <div className="stat">
            <span className="v" style={{ color: session.failed ? "var(--bad)" : undefined }}>
              {session.failed}
            </span>
            <span className="k">failed</span>
          </div>
          <div className="stat">
            <span className="v">{session.processed}</span>
            <span className="k">processed</span>
          </div>
        </div>

        <div className="actionbar" style={{ marginTop: "var(--space-4)" }}>
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
        <WorkerBars active={active} minSavings={store.minSavings} />
      </div>

      <div className="card">
        <div className="card-title">Event log</div>
        <EventLog log={store.log} />
      </div>
    </div>
  );
}
