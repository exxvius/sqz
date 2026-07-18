import { StatusCard } from "./StatusCard";
import { FolderIcon, PlayIcon } from "./icons";
import { openFile, revealFile } from "../lib/api";
import type { LogEntry } from "../lib/store";
import { currentPath, humanBytes } from "../lib/format";
import { outcomeMeta } from "../lib/status";

interface Props {
  log: LogEntry[];
  onRetry: (path: string) => void;
  onForce: (path: string) => void;
}

export function EventLog({ log, onRetry, onForce }: Props) {
  if (log.length === 0) {
    return <div className="empty">Events appear here as files are processed.</div>;
  }

  // Only the most recent entries are rendered (log is newest-first); thousands
  // of cards would bog the page down. Older events remain in the History tab.
  return (
    <div>
      {log.slice(0, 25).map((e, i) => {
        const m = outcomeMeta(e.outcome);
        const isFail = e.outcome === "failed";
        const isSkip = e.outcome.startsWith("skipped");
        const encoded = e.outcome === "done" || e.outcome === "normalized";
        const filePath = currentPath(e.path, encoded);
        const meta =
          e.outcome === "done" || e.outcome === "normalized" ? (
            <span className="saved-tag">−{humanBytes(e.savedBytes)}</span>
          ) : null;

        const actions = (
          <>
            {!isFail && (
              <>
                <button className="mini-btn" onClick={() => openFile(filePath)}>
                  <PlayIcon /> Open
                </button>
                <button className="mini-btn" onClick={() => revealFile(filePath)}>
                  <FolderIcon /> Folder
                </button>
              </>
            )}
            {isFail && (
              <button className="mini-btn" onClick={() => onRetry(e.path)}>
                ↻ Retry
              </button>
            )}
            {isSkip && (
              <button className="mini-btn" onClick={() => onForce(e.path)}>
                ⏵ Force process
              </button>
            )}
          </>
        );

        return (
          <StatusCard
            key={`${e.path}-${i}`}
            tone={m.tone}
            sym={m.sym}
            name={e.name}
            fullPath={e.path}
            tag={m.label}
            meta={meta}
            actions={actions}
          >
            <dl className="kv-grid">
              <dt>path</dt>
              <dd>{e.path}</dd>
              {e.origSize != null && (
                <>
                  <dt>before</dt>
                  <dd>{humanBytes(e.origSize)}</dd>
                </>
              )}
              {e.outSize != null && (
                <>
                  <dt>after</dt>
                  <dd>{humanBytes(e.outSize)}</dd>
                </>
              )}
              {(e.outcome === "done" || e.outcome === "normalized") && (
                <>
                  <dt>saved</dt>
                  <dd>{humanBytes(e.savedBytes)}</dd>
                </>
              )}
            </dl>

            {isFail && e.message && <div className="err-box">{e.message}</div>}
            {!isFail && e.message && <p className="muted">{e.message}</p>}
          </StatusCard>
        );
      })}
    </div>
  );
}
