# Screenshot harness

Generates the images in [`../docs/images/`](../docs/images) — the README banner and
the app screenshots — programmatically, so they can be regenerated whenever the UI
changes instead of being captured by hand.

It works by running the **real** React frontend (`../src/App`) in headless Chromium
with the Tauri backend mocked. Tauri's own `mockIPC(handler, { shouldMockEvents: true })`
stands in for the `invoke`/`listen` transport, so canned command responses and scripted
engine events drive the actual UI components — these are genuine captures, not mockups.

## Files

| File | Purpose |
|------|---------|
| `harness.html` / `harness.tsx` | Mounts `src/App` with the IPC/event layer mocked. Selected via `?scene=` and `?theme=`. |
| `scenes.ts` | Canned `invoke` responses + scripted `sqz-*` engine events per scene. |
| `banner.html` | The README header, rendered to PNG (uses the app's self-hosted DM Sans). |
| `capture.mjs` | Drives the harness in Playwright and writes the app screenshots. |
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
node screenshots/capture.mjs   # dashboard (dark+light), home, history
node screenshots/banner.mjs    # banner
```

Screenshots are captured at 1440×900, 2× device scale.

## Adding a scene

1. Add canned responses / an event script to `scenes.ts`.
2. Add an entry to `SHOTS` in `capture.mjs` (scene, theme, output file, and a `wait`
   selector that only appears once the scene has rendered).

The mocking seam stays small because the whole app funnels through one `invoke`
wrapper (`src/lib/api.ts`) and one `listen` wrapper (`src/lib/events.ts`).
