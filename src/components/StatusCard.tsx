import { useState, type ReactNode } from "react";
import type { Tone } from "../lib/status";

interface Props {
  tone: Tone;
  sym: string;
  name: string;
  fullPath?: string;
  tag?: string;
  meta?: ReactNode;
  /** Buttons shown in the card header at all times (collapsed or expanded). */
  actions?: ReactNode;
  /** Detail shown only when expanded. */
  children: ReactNode;
  defaultOpen?: boolean;
}

/** An expandable, color-tinted card used by the event log and history. */
export function StatusCard({
  tone,
  sym,
  name,
  fullPath,
  tag,
  meta,
  actions,
  children,
  defaultOpen,
}: Props) {
  const [open, setOpen] = useState(defaultOpen ?? false);
  return (
    <div className={`ecard tone-${tone}${open ? " open" : ""}`}>
      <button className="ecard-head" onClick={() => setOpen((o) => !o)}>
        <span className="ecard-sym">{sym}</span>
        <span className="ecard-name" title={fullPath ?? name}>
          {name}
        </span>
        {tag && <span className="ecard-tag">{tag}</span>}
        {meta && <span className="ecard-meta">{meta}</span>}
        <span className="ecard-caret" aria-hidden>
          ›
        </span>
      </button>

      {actions && <div className="ecard-actions">{actions}</div>}
      <div className={`ecard-collapse${open ? " open" : ""}`}>
        <div className="ecard-collapse-inner">
          <div className="ecard-body">{children}</div>
        </div>
      </div>
    </div>
  );
}
