# Screenshot harness

Generates the images in [`../docs/images/`](../docs/images) — the README banner, the
dashboard hero, and the per-feature marketing panels — programmatically, so they can
be regenerated whenever the UI changes instead of being captured by hand.

Everything runs the **real** React frontend (`../src/App`) in headless Chromium with
the Tauri backend mocked. Tauri's own `mockIPC(handler, { shouldMockEvents: true })`
stands in for the `invoke`/`listen` transport, so canned command responses and scripted
engine events drive the actual UI components — these are genuine captures, not mockups.

## Two generators

- **`capture.mjs`** — the full-window hero shots (`dashboard-dark`/`-light`).
- **`marketing.mjs`** — the per-feature panels (`feature-*.png`). Each is composed
  from real components: it grabs one or more elements as **transparent-background
  pieces** (element screenshots with the app background stripped), then arranges
  them — a fanned stack of parallel encodes, a receding history deck, a bento of the
  lock controls, the accent picker sliced into five colours, the Live page split on a
  tilted light/dark seam — and screenshots a transparent stage. Every piece is
  re-clipped to its own rounded-rectangle shape when placed, which removes the faint
  bounding-box-corner artifacts the raw element screenshots leave behind. The panels
  carry no backdrop or shadow of their own; they sit on the README's background.

## Files

| File | Purpose |
|------|---------|
| `harness.html` / `harness.tsx` | Mounts `src/App` with the IPC/event layer mocked. Selected via `?scene=`, `?theme=`, `?locked=`. |
| `scenes.ts` | Canned `invoke` responses + scripted `sqz-*` engine events per scene. |
| `banner.html` | The README header, rendered to PNG (uses the app's self-hosted DM Sans). |
| `capture.mjs` | The full-window hero shots. |
| `marketing.mjs` | The composed per-feature panels. |
| `banner.mjs` | Renders `banner.html` to `banner.png`. |

## Regenerate

Playwright is intentionally **not** a project dependency (keeps the app tiny). Install
it on demand:

```bash
# 1. one-time, if not already present
npm install --no-save playwright
npx playwright install chromium

# 2. start the frontend dev server (Vite on :1420)
npm run dev

# 3. in another terminal, capture
node screenshots/capture.mjs     # dashboard hero (dark + light)
node screenshots/marketing.mjs   # every feature-*.png panel
node screenshots/banner.mjs      # banner
```

The page renders at 2× device scale; hero/full-bleed shots are 1440×900, and the
composed panels are sized to their content.

## Adding a panel

1. If it needs new data, add canned responses / an event script to `scenes.ts`.
2. In `marketing.mjs`, write a function that `open()`s a scene, `grab()`s the
   pieces it needs (`opaque` for solid stacked cards, `strip` for content without a
   card frame), arranges them with `place()` (which clips each piece to its rounded
   shape), and `save()`s the `stage()` output. Add a call to it at the bottom.

The mocking seam stays small because the whole app funnels through one `invoke`
wrapper (`src/lib/api.ts`) and one `listen` wrapper (`src/lib/events.ts`).
