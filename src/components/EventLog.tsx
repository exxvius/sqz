import type { LogEntry } from "../lib/store";
import { humanBytes } from "../lib/format";
import type { Outcome } from "../lib/types";

const SYMBOL: Record<Outcome, { sym: string; klass: string }> = {
  done: { sym: "✓", klass: "done" },
  skipped_efficient: { sym: "»", klass: "skip" },
  skipped_marginal: { sym: "~", klass: "skip" },
  skipped_no_gain: { sym: "=", klass: "skip" },
  failed: { sym: "✗", klass: "fail" },
  cancelled: { sym: "•", klass: "skip" },
  dry_run: { sym: "·", klass: "skip" },
};

function describe(e: LogEntry): string {
  switch (e.outcome) {
    case "done":
      return `${humanBytes(e.origSize)} → ${humanBytes(e.outSize)} · saved ${humanBytes(
        e.savedBytes,
      )}`;
    case "skipped_efficient":
      return "already efficient";
    case "skipped_marginal":
      return "already lean — skipped";
    case "skipped_no_gain":
      return e.message || "no meaningful gain — original kept";
    case "failed":
      return e.message || "failed";
    default:
      return e.message;
  }
}

export function EventLog({ log }: { log: LogEntry[] }) {
  if (log.length === 0) {
    return <div className="empty">Events will appear here as files are processed.</div>;
  }
  return (
    <div className="log" role="log" aria-live="polite">
      {log.map((e, i) => {
        const s = SYMBOL[e.outcome];
        return (
          <div className="log-row" key={`${e.path}-${i}`}>
            <span className={`sym ${s.klass}`}>{s.sym}</span>
            <span className="lname" title={e.path}>
              {e.name}
            </span>
            <span className="lmsg">{describe(e)}</span>
          </div>
        );
      })}
    </div>
  );
}
