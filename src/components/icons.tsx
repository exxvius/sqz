// Minimal stroked line icons (currentColor), 20×20 on a 24 viewBox.

interface IconProps {
  size?: number;
}

const base = (size: number) => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
});

export function HomeIcon({ size = 18 }: IconProps) {
  return (
    <svg {...base(size)}>
      <path d="M3 10.5 12 3l9 7.5" />
      <path d="M5 9.5V20h14V9.5" />
      <path d="M9.5 20v-6h5v6" />
    </svg>
  );
}

export function LiveIcon({ size = 18 }: IconProps) {
  return (
    <svg {...base(size)}>
      <path d="M3 12h3l2.5 7 5-16 2.5 9H21" />
    </svg>
  );
}

export function HistoryIcon({ size = 18 }: IconProps) {
  return (
    <svg {...base(size)}>
      <path d="M3.5 9a9 9 0 1 1-1.2 5" />
      <path d="M2 4v5h5" />
      <path d="M12 8v4l3 2" />
    </svg>
  );
}

export function SettingsIcon({ size = 18 }: IconProps) {
  return (
    <svg {...base(size)}>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1" />
    </svg>
  );
}

export function FolderIcon({ size = 13 }: IconProps) {
  return (
    <svg {...base(size)}>
      <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2h8.5A1.5 1.5 0 0 1 21 8.5V17a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
    </svg>
  );
}

export function PlayIcon({ size = 13 }: IconProps) {
  return (
    <svg {...base(size)}>
      <path d="M7 5.5 18 12 7 18.5Z" />
    </svg>
  );
}

/** The sqz mark: two arrows squeezing toward a bar. */
export function Logo({ size = 22 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
      <rect x="1" y="1" width="22" height="22" rx="6" fill="var(--accent-quiet)" />
      <g stroke="var(--accent)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" fill="none">
        <path d="M7 7 12 10.6 17 7" />
        <path d="M7 17 12 13.4 17 17" />
      </g>
      <rect x="7" y="11.2" width="10" height="1.6" rx="0.8" fill="var(--accent)" />
    </svg>
  );
}
