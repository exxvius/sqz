// Filled icons (currentColor) on a 24×24 viewBox.

interface IconProps {
  size?: number;
}

// Filled nav icons (currentColor) on a 24 viewBox.
const filled = (size: number) => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "currentColor" as const,
  "aria-hidden": true,
});

export function HomeIcon({ size = 18 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M9 13h6v6h3v-9l-6-4.5L6 10v9h3zm-5 8V9l8-6l8 6v12z" />
    </svg>
  );
}

export function LiveIcon({ size = 18 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M13 2.03v2.02c4.39.54 7.5 4.53 6.96 8.92c-.46 3.64-3.32 6.53-6.96 6.96v2c5.5-.55 9.5-5.43 8.95-10.93c-.45-4.75-4.22-8.5-8.95-8.97m-2 .03c-1.95.19-3.81.94-5.33 2.2L7.1 5.74c1.12-.9 2.47-1.48 3.9-1.68zM4.26 5.67A9.9 9.9 0 0 0 2.05 11h2c.19-1.42.75-2.77 1.64-3.9zM2.06 13c.2 1.96.97 3.81 2.21 5.33l1.42-1.43A8 8 0 0 1 4.06 13zm5.04 5.37l-1.43 1.37A10 10 0 0 0 11 22v-2a8 8 0 0 1-3.9-1.63M12.5 7v5.25l4.5 2.67l-.75 1.23L11 13V7z" />
    </svg>
  );
}

export function HistoryIcon({ size = 18 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M13.5 8H12v5l4.28 2.54l.72-1.21l-3.5-2.08zM13 3a9 9 0 0 0-9 9H1l3.96 4.03L9 12H6a7 7 0 0 1 7-7a7 7 0 0 1 7 7a7 7 0 0 1-7 7c-1.93 0-3.68-.79-4.94-2.06l-1.42 1.42A8.9 8.9 0 0 0 13 21a9 9 0 0 0 9-9a9 9 0 0 0-9-9" />
    </svg>
  );
}

export function LibraryIcon({ size = 18 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="m11.5 14.5l7-4.5l-7-4.5zM8 18q-.825 0-1.412-.587T6 16V4q0-.825.588-1.412T8 2h12q.825 0 1.413.588T22 4v12q0 .825-.587 1.413T20 18zm0-2h12V4H8zm-4 6q-.825 0-1.412-.587T2 20V6h2v14h14v2zM8 4v12z" />
    </svg>
  );
}

/** Library with a plus — the "new saved library" action. */
export function NewLibraryIcon({ size = 18 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M14.713 13.713Q15 13.425 15 13v-2h2q.425 0 .713-.288T18 10t-.288-.712T17 9h-2V7q0-.425-.288-.712T14 6t-.712.288T13 7v2h-2q-.425 0-.712.288T10 10t.288.713T11 11h2v2q0 .425.288.713T14 14t.713-.288M8 18q-.825 0-1.412-.587T6 16V4q0-.825.588-1.412T8 2h12q.825 0 1.413.588T22 4v12q0 .825-.587 1.413T20 18zm0-2h12V4H8zm-4 6q-.825 0-1.412-.587T2 20V7q0-.425.288-.712T3 6t.713.288T4 7v13h13q.425 0 .713.288T18 21t-.288.713T17 22zM8 4v12z" />
    </svg>
  );
}

/** Bracketed eye — the "watch this library" (unattended) toggle. */
export function WatchIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M1 23v-5h2v3h3v2zm17 0v-2h3v-3h2v5zm-6-4.5q-3 0-5.437-1.775T3 12q1.125-2.95 3.563-4.725T12 5.5t5.438 1.775T21 12q-1.125 2.95-3.562 4.725T12 18.5m0-2q2.2 0 4.025-1.2t2.8-3.3q-.975-2.1-2.8-3.3T12 7.5T7.975 8.7t-2.8 3.3q.975 2.1 2.8 3.3T12 16.5m0-1q1.45 0 2.475-1.025T15.5 12t-1.025-2.475T12 8.5T9.525 9.525T8.5 12t1.025 2.475T12 15.5m0-2q-.625 0-1.063-.437T10.5 12t.438-1.062T12 10.5t1.063.438T13.5 12t-.437 1.063T12 13.5M1 6V1h5v2H3v3zm20 0V3h-3V1h5v5z" />
    </svg>
  );
}

/** Circular arrow — retry a failed file. */
export function RetryIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M12 20q-3.35 0-5.675-2.325T4 12t2.325-5.675T12 4q1.725 0 3.3.712T18 6.75V4h2v7h-7V9h4.2q-.8-1.4-2.187-2.2T12 6Q9.5 6 7.75 7.75T6 12t1.75 4.25T12 18q1.925 0 3.475-1.1T17.65 14h2.1q-.7 2.65-2.85 4.325T12 20" />
    </svg>
  );
}

/** Double chevron — force a skipped file through an encode. */
export function ForceIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M9.575 12L5 7.4L6.4 6l6 6l-6 6L5 16.6zm6.6 0L11.6 7.4L13 6l6 6l-6 6l-1.4-1.4z" />
    </svg>
  );
}

/** Counter-clockwise arrow around a dot — restore the original. */
export function RestoreIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M12 14q-.825 0-1.412-.587T10 12t.588-1.412T12 10t1.413.588T14 12t-.587 1.413T12 14m0 7q-3.475 0-6.025-2.287T3.05 13H5.1q.35 2.6 2.313 4.3T12 19q2.925 0 4.963-2.037T19 12t-2.037-4.962T12 5q-1.725 0-3.225.8T6.25 8H9v2H3V4h2v2.35q1.275-1.6 3.113-2.475T12 3q1.875 0 3.513.713t2.85 1.924t1.925 2.85T21 12t-.712 3.513t-1.925 2.85t-2.85 1.925T12 21" />
    </svg>
  );
}

/** Trash can — remove a row from a list. */
export function RemoveIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M7 21q-.825 0-1.412-.587T5 19V6H4V4h5V3h6v1h5v2h-1v13q0 .825-.587 1.413T17 21zM17 6H7v13h10zM9 17h2V8H9zm4 0h2V8h-2zM7 6v13z" />
    </svg>
  );
}

/** Box with an out-arrow — export to a file. */
export function ExportIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="m5.05 22.375l-1.4-1.425L6.6 18H4.35v-2H10v5.65H8v-2.225zM12 22v-2h6V9h-5V4H6v10H4V4q0-.825.588-1.412T6 2h8l6 6v12q0 .825-.587 1.413T18 22z" />
    </svg>
  );
}

/** X in a circle — cancel an in-progress action. */
export function CancelIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="m8.4 17l3.6-3.6l3.6 3.6l1.4-1.4l-3.6-3.6L17 8.4L15.6 7L12 10.6L8.4 7L7 8.4l3.6 3.6L7 15.6zm3.6 5q-2.075 0-3.9-.788t-3.175-2.137T2.788 15.9T2 12t.788-3.9t2.137-3.175T8.1 2.788T12 2t3.9.788t3.175 2.137T21.213 8.1T22 12t-.788 3.9t-2.137 3.175t-3.175 2.138T12 22m0-2q3.35 0 5.675-2.325T20 12t-2.325-5.675T12 4T6.325 6.325T4 12t2.325 5.675T12 20m0-8" />
    </svg>
  );
}

/** Small X — remove an entry from a queue/list being built. */
export function RemoveXIcon({ size = 14 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="m8.382 17.025l-1.407-1.4L10.593 12L6.975 8.4L8.382 7L12 10.615L15.593 7L17 8.4L13.382 12L17 15.625l-1.407 1.4L12 13.41z" />
    </svg>
  );
}

/** Stacked lines — clear a list. */
export function ClearIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M3 17v-2h14v2zm2-4v-2h14v2zm2-4V7h14v2z" />
    </svg>
  );
}

/** Folder with a plus — add a folder. */
export function AddFolderIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M2 20V4h8l2 2h10v14zm2-2h16V8h-8.825l-2-2H4zm0 0V6zm10-2h2v-2h2v-2h-2v-2h-2v2h-2v2h2z" />
    </svg>
  );
}

/** Box with an in-arrow — import from a file. */
export function ImportIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M11 19h2v-4.175l1.6 1.6L16 15l-4-4l-4 4l1.425 1.4L11 14.825zm-7 3V2h10l6 6v14zm9-13V4H6v16h12V9zM6 4v5zv16z" />
    </svg>
  );
}

/** Floppy disk — save. */
export function SaveIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M21 7v14H3V3h14zm-2 .85L16.15 5H5v14h14zm-4.875 9.275Q15 16.25 15 15t-.875-2.125T12 12t-2.125.875T9 15t.875 2.125T12 18t2.125-.875M6 10h9V6H6zM5 7.85V19V5z" />
    </svg>
  );
}

/** Checkmark — a healthy / re-encoded status symbol. */
export function CheckIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="m9.55 18l-5.7-5.7l1.425-1.425L9.55 15.15l9.175-9.175L20.15 7.4z" />
    </svg>
  );
}

/** Exclamation — a warning / playback-caveat status symbol. */
export function AlertIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M11 14V5h2v9zm0 5v-2h2v2z" />
    </svg>
  );
}

/** Magnifying glass — the health scan action. */
export function SearchIcon({ size = 13 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M18.319 14.433A8.001 8.001 0 0 0 6.343 3.868a8 8 0 0 0 10.564 11.976l.043.045l4.242 4.243a1 1 0 1 0 1.415-1.415l-4.243-4.242zm-2.076-9.15a6 6 0 1 1-8.485 8.485a6 6 0 0 1 8.485-8.485"
      />
    </svg>
  );
}

/** Magnifying glass with a focused core — the deep (decode) scan action. */
export function DeepScanIcon({ size = 13 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M5 11a6 6 0 1 1 12 0a6 6 0 0 1-12 0m6-8a8 8 0 1 0 4.906 14.32l3.387 3.387a1 1 0 0 0 1.414-1.414l-3.387-3.387A8 8 0 0 0 11 3m0 12a4 4 0 1 0 0-8a4 4 0 0 0 0 8"
      />
    </svg>
  );
}

/** Cross — a failed / corrupt / unreadable status symbol. */
export function FailIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M6.4 19L5 17.6l5.6-5.6L5 6.4L6.4 5l5.6 5.6L17.6 5L19 6.4L13.4 12l5.6 5.6l-1.4 1.4l-5.6-5.6z" />
    </svg>
  );
}

/** Two curved arrows — a container-normalized (remuxed) status symbol. */
export function NormalizedIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M4 20v-2h2.75l-.4-.35q-1.225-1.225-1.787-2.662T4 12.05q0-2.775 1.663-4.937T10 4.25v2.1Q8.2 7 7.1 8.563T6 12.05q0 1.125.425 2.188T7.75 16.2l.25.25V14h2v6zm10-.25v-2.1q1.8-.65 2.9-2.212T18 11.95q0-1.125-.425-2.187T16.25 7.8L16 7.55V10h-2V4h6v2h-2.75l.4.35q1.225 1.225 1.788 2.663T20 11.95q0 2.775-1.662 4.938T14 19.75" />
    </svg>
  );
}

/** Skip-forward bar — an already-efficient status symbol. */
export function EfficientIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M16.5 18V6h2v12zm-11 0V6l9 6z" />
    </svg>
  );
}

/** Equals bars — a no-gain / kept-original status symbol. */
export function NoGainIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M4 17v-3h16v3zm0-7V7h16v3z" />
    </svg>
  );
}

/** Spinning arc — an in-progress status symbol. Uses a CSS animation (not SMIL)
 *  so it starts spinning the instant it renders, with no first-frame delay. */
export function ProcessingIcon({ size = 16 }: IconProps) {
  return (
    <svg
      className="spin"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden
    >
      <path
        d="M12 3c4.97 0 9 4.03 9 9"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** Wave — a lean / marginal (skipped) status symbol. */
export function LeanIcon({ size = 16 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 256 256"
      fill="currentColor"
      aria-hidden
    >
      <path d="M225.35 133.1c-15.22 18.93-30.43 29-46.5 30.65a47 47 0 0 1-4.85.25c-20.81 0-38.16-14.13-53.59-26.7c-14.24-11.6-27.68-22.54-40.75-21.18c-9.26 1-19.46 8.32-30.32 21.82a12 12 0 0 1-18.7-15C45.87 104 61.08 94 77.15 92.25c23-2.42 41.82 12.92 58.43 26.45c14.24 11.6 27.68 22.54 40.75 21.18c9.26-1 19.46-8.32 30.32-21.82a12 12 0 1 1 18.7 15Z" />
    </svg>
  );
}

/** Small hollow ring — a pending status symbol. */
export function PendingIcon({ size = 16 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 15 15"
      fill="currentColor"
      aria-hidden
    >
      <path d="M7.5 4.875a2.625 2.625 0 1 1 0 5.25a2.625 2.625 0 0 1 0-5.25m0 1a1.625 1.625 0 1 0 0 3.25a1.625 1.625 0 0 0 0-3.25" />
    </svg>
  );
}

/** Small solid dot — a dry-run ("would encode") status symbol. */
export function DryRunIcon({ size = 16 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 15 15"
      fill="currentColor"
      aria-hidden
    >
      <path d="M4.5 7.5a3 3 0 1 0 6 0a3 3 0 1 0-6 0" />
    </svg>
  );
}

/** Solid dot — a not-yet-scanned library status symbol. */
export function NotScannedIcon({ size = 16 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 48 48"
      fill="currentColor"
      aria-hidden
    >
      <path
        stroke="currentColor"
        strokeWidth={4}
        d="M24 33a9 9 0 1 0 0-18a9 9 0 0 0 0 18Z"
      />
    </svg>
  );
}

/** Cross in a circle — a cancelled status symbol. */
export function CancelledIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="m8.4 17l3.6-3.6l3.6 3.6l1.4-1.4l-3.6-3.6L17 8.4L15.6 7L12 10.6L8.4 7L7 8.4l3.6 3.6L7 15.6zm3.6 5q-2.075 0-3.9-.788t-3.175-2.137T2.788 15.9T2 12t.788-3.9t2.137-3.175T8.1 2.788T12 2t3.9.788t3.175 2.137T21.213 8.1T22 12t-.788 3.9t-2.137 3.175t-3.175 2.138T12 22m0-2q3.35 0 5.675-2.325T20 12t-2.325-5.675T12 4T6.325 6.325T4 12t2.325 5.675T12 20" />
    </svg>
  );
}

export function SettingsIcon({ size = 18 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M12 8a4 4 0 0 1 4 4a4 4 0 0 1-4 4a4 4 0 0 1-4-4a4 4 0 0 1 4-4m0 2a2 2 0 0 0-2 2a2 2 0 0 0 2 2a2 2 0 0 0 2-2a2 2 0 0 0-2-2m-2 12c-.25 0-.46-.18-.5-.42l-.37-2.65c-.63-.25-1.17-.59-1.69-.99l-2.49 1.01c-.22.08-.49 0-.61-.22l-2-3.46a.493.493 0 0 1 .12-.64l2.11-1.66L4.5 12l.07-1l-2.11-1.63a.493.493 0 0 1-.12-.64l2-3.46c.12-.22.39-.31.61-.22l2.49 1c.52-.39 1.06-.73 1.69-.98l.37-2.65c.04-.24.25-.42.5-.42h4c.25 0 .46.18.5.42l.37 2.65c.63.25 1.17.59 1.69.98l2.49-1c.22-.09.49 0 .61.22l2 3.46c.13.22.07.49-.12.64L19.43 11l.07 1l-.07 1l2.11 1.63c.19.15.25.42.12.64l-2 3.46c-.12.22-.39.31-.61.22l-2.49-1c-.52.39-1.06.73-1.69.98l-.37 2.65c-.04.24-.25.42-.5.42zm1.25-18l-.37 2.61c-1.2.25-2.26.89-3.03 1.78L5.44 7.35l-.75 1.3L6.8 10.2a5.55 5.55 0 0 0 0 3.6l-2.12 1.56l.75 1.3l2.43-1.04c.77.88 1.82 1.52 3.01 1.76l.37 2.62h1.52l.37-2.61c1.19-.25 2.24-.89 3.01-1.77l2.43 1.04l.75-1.3l-2.12-1.55c.4-1.17.4-2.44 0-3.61l2.11-1.55l-.75-1.3l-2.41 1.04a5.42 5.42 0 0 0-3.03-1.77L12.75 4z" />
    </svg>
  );
}

export function FolderIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M2 20V4h8l2 2h10v14zm2-2h16V8h-8.825l-2-2H4zm0 0V6z" />
    </svg>
  );
}

export function PlayIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M8 19V5l11 7zm2-3.65L15.25 12L10 8.65z" />
    </svg>
  );
}

/** Pencil — edit a saved library. */
export function EditIcon({ size = 15 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M5 19h1.425L16.2 9.225L14.775 7.8L5 17.575zm-2 2v-4.25L17.625 2.175L21.8 6.45L7.25 21zM19 6.4L17.6 5zm-3.525 2.125l-.7-.725L16.2 9.225z" />
    </svg>
  );
}

/** Closed padlock — the app is locked. */
export function LockIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M6 22q-.825 0-1.412-.587T4 20V10q0-.825.588-1.412T6 8h1V6q0-2.075 1.463-3.537T12 1t3.538 1.463T17 6v2h1q.825 0 1.413.588T20 10v10q0 .825-.587 1.413T18 22zm7.413-5.587Q14 15.825 14 15t-.587-1.412T12 13t-1.412.588T10 15t.588 1.413T12 17t1.413-.587M9 8h6V6q0-1.25-.875-2.125T12 3t-2.125.875T9 6z" />
    </svg>
  );
}

/** Open padlock — the app is unlocked. */
export function UnlockIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M6 8h9V6q0-1.25-.875-2.125T12 3t-2.125.875T9 6H7q0-2.075 1.463-3.537T12 1t3.538 1.463T17 6v2h1q.825 0 1.413.588T20 10v10q0 .825-.587 1.413T18 22H6q-.825 0-1.412-.587T4 20V10q0-.825.588-1.412T6 8m7.413 8.413Q14 15.825 14 15t-.587-1.412T12 13t-1.412.588T10 15t.588 1.413T12 17t1.413-.587" />
    </svg>
  );
}

/** Crescent moon — dark theme active. */
export function MoonIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M12 21q-3.775 0-6.387-2.613T3 12q0-3.45 2.25-5.988T11 3.05q.325-.05.575.088t.4.362t.163.525t-.188.575q-.425.65-.638 1.375T11.1 7.5q0 2.25 1.575 3.825T16.5 12.9q.775 0 1.538-.225t1.362-.625q.275-.175.563-.162t.512.137q.25.125.388.375t.087.6q-.35 3.45-2.937 5.725T12 21" />
    </svg>
  );
}

/** Sun — light theme active. */
export function SunIcon({ size = 16 }: IconProps) {
  return (
    <svg {...filled(size)}>
      <path d="M8.463 15.538Q7 14.075 7 12t1.463-3.537T12 7t3.538 1.463T17 12t-1.463 3.538T12 17t-3.537-1.463M2 13q-.425 0-.712-.288T1 12t.288-.712T2 11h2q.425 0 .713.288T5 12t-.288.713T4 13zm18 0q-.425 0-.712-.288T19 12t.288-.712T20 11h2q.425 0 .713.288T23 12t-.288.713T22 13zm-8.712-8.287Q11 4.425 11 4V2q0-.425.288-.712T12 1t.713.288T13 2v2q0 .425-.288.713T12 5t-.712-.288m0 18Q11 22.426 11 22v-2q0-.425.288-.712T12 19t.713.288T13 20v2q0 .425-.288.713T12 23t-.712-.288M5.65 7.05L4.575 6q-.3-.275-.288-.7t.288-.725q.3-.3.725-.3t.7.3L7.05 5.65q.275.3.275.7t-.275.7t-.687.288t-.713-.288M18 19.425l-1.05-1.075q-.275-.3-.275-.712t.275-.688q.275-.3.688-.287t.712.287L19.425 18q.3.275.288.7t-.288.725q-.3.3-.725.3t-.7-.3M16.95 7.05q-.3-.275-.288-.687t.288-.713L18 4.575q.275-.3.7-.288t.725.288q.3.3.3.725t-.3.7L18.35 7.05q-.3.275-.7.275t-.7-.275M4.575 19.425q-.3-.3-.3-.725t.3-.7l1.075-1.05q.3-.275.712-.275t.688.275q.3.275.288.688t-.288.712L6 19.425q-.275.3-.7.288t-.725-.288" />
    </svg>
  );
}

/** The sqz mark: two arrows squeezing toward a bar. */
export function Logo({ size = 22 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
      <rect
        x="1"
        y="1"
        width="22"
        height="22"
        rx="6"
        fill="var(--accent-quiet)"
      />
      <g
        stroke="var(--accent)"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      >
        <path d="M7 7 12 10.6 17 7" />
        <path d="M7 17 12 13.4 17 17" />
      </g>
      <rect
        x="7"
        y="11.2"
        width="10"
        height="1.6"
        rx="0.8"
        fill="var(--accent)"
      />
    </svg>
  );
}
