import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { pickInputs } from "../lib/api";

interface Props {
  onAdd: (paths: string[]) => void;
}

/**
 * Drag-and-drop target for files/folders, backed by Tauri's native file-drop
 * event (browser drag events don't carry real filesystem paths). Also offers
 * explicit "Add files/folders" buttons via the native picker.
 */
export function DropZone({ onAdd }: Props) {
  const [over, setOver] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over" || event.payload.type === "enter") {
          setOver(true);
        } else if (event.payload.type === "drop") {
          setOver(false);
          onAdd(event.payload.paths);
        } else {
          setOver(false);
        }
      })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  }, [onAdd]);

  return (
    <div className={`dropzone${over ? " over" : ""}`}>
      <div className="big">Drop videos or folders here</div>
      <div className="muted">
        Every file is verified before its original is touched. Nothing is deleted without a smaller,
        playable replacement.
      </div>
      <div className="dz-actions">
        <button className="btn" onClick={async () => onAdd(await pickInputs(false))}>
          Add files
        </button>
        <button className="btn ghost" onClick={async () => onAdd(await pickInputs(true))}>
          Add folders
        </button>
      </div>
    </div>
  );
}
