import { useState, type ReactNode } from "react";

interface Props {
  title: string;
  children: ReactNode;
  defaultOpen?: boolean;
}

/** A header + animated expand/collapse body (height transitions to auto). */
export function Collapsible({ title, children, defaultOpen }: Props) {
  const [open, setOpen] = useState(defaultOpen ?? false);
  return (
    <div className={`clps${open ? " open" : ""}`}>
      <button className="clps-head" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <span className="clps-caret" aria-hidden>
          ›
        </span>
        <span>{title}</span>
      </button>
      <div className="clps-body">
        <div className="clps-body-inner">{children}</div>
      </div>
    </div>
  );
}
