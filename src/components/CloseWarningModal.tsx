interface Props {
  onQuit: () => void;
  onMinimize: () => void;
  onCancel: () => void;
}

/** Shown when the window is closed while a run is in progress. */
export function CloseWarningModal({ onQuit, onMinimize, onCancel }: Props) {
  return (
    <div
      className="overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="close-warn-title"
      onClick={onCancel}
    >
      <div className="card confirm-modal" onClick={(e) => e.stopPropagation()}>
        <h2 id="close-warn-title">Files are still processing</h2>
        <p className="muted">
          A run is in progress. Quitting now stops it — progress is saved and the run is resumable,
          but in-flight encodes are cancelled. You can minimize to the tray to keep encoding instead.
        </p>
        <div className="confirm-actions">
          <button className="btn ghost" onClick={onCancel}>
            Cancel
          </button>
          <button className="btn" onClick={onMinimize} autoFocus>
            Minimize to tray
          </button>
          <button className="btn danger" onClick={onQuit}>
            Quit anyway
          </button>
        </div>
      </div>
    </div>
  );
}
