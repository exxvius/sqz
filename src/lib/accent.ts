import { useEffect, useState } from "react";

/** Accent presets (must match the `[data-accent="…"]` rules in tokens.css). */
export type Accent =
  | "teal"
  | "sky"
  | "blue"
  | "violet"
  | "magenta"
  | "rose"
  | "amber"
  | "green";

export interface AccentOption {
  id: Accent;
  label: string;
  /** A representative swatch color (dark-mode approximation) for the picker. */
  swatch: string;
}

export const ACCENTS: AccentOption[] = [
  { id: "teal", label: "Teal", swatch: "oklch(80% 0.13 195)" },
  { id: "sky", label: "Sky", swatch: "oklch(80% 0.13 230)" },
  { id: "blue", label: "Blue", swatch: "oklch(78% 0.15 255)" },
  { id: "violet", label: "Violet", swatch: "oklch(78% 0.15 292)" },
  { id: "magenta", label: "Magenta", swatch: "oklch(78% 0.15 330)" },
  { id: "rose", label: "Rose", swatch: "oklch(78% 0.16 15)" },
  { id: "amber", label: "Amber", swatch: "oklch(82% 0.14 70)" },
  { id: "green", label: "Green", swatch: "oklch(80% 0.15 150)" },
];

const KEY = "sqz-accent";
const DEFAULT: Accent = "teal";

function isAccent(v: string | null): v is Accent {
  return ACCENTS.some((a) => a.id === v);
}

export function useAccent(): [Accent, (a: Accent) => void] {
  const [accent, setAccent] = useState<Accent>(() => {
    const stored = localStorage.getItem(KEY);
    return isAccent(stored) ? stored : DEFAULT;
  });

  useEffect(() => {
    document.documentElement.setAttribute("data-accent", accent);
    localStorage.setItem(KEY, accent);
  }, [accent]);

  return [accent, setAccent];
}
