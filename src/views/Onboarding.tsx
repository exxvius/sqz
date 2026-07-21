interface Props {
  onClose: () => void;
}

export function Onboarding({ onClose }: Props) {
  return (
    <div
      className="overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ob-title"
    >
      <div className="card onboard">
        <h2 id="ob-title">sqz</h2>
        <p className="muted">
          Squeeze your video library. Reclaim disk space. Never lose a file.
        </p>
        <ol>
          <li>Add videos or folders — files or whole libraries.</li>
          <li>
            Pick a codec and a quality preset. Auto-detected hardware does the
            heavy lifting.
          </li>
          <li>
            Press <strong>Start</strong>. Each file is verified before its
            original is ever touched.
          </li>
          <li>
            Originals go to the Recycle Bin by default, so mistakes are always
            recoverable.
          </li>
        </ol>
        <button className="btn primary lg" onClick={onClose}>
          Get started
        </button>
      </div>
    </div>
  );
}
