// Renders the real <App/> against a mocked Tauri IPC + event transport so the
// UI can be screenshotted headlessly. Driven by ?scene= and ?theme= params.

import ReactDOM from "react-dom/client";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "../src/App";
import "../src/styles/fonts.css";
import "../src/styles/global.css";
import "../src/styles/app.css";
import { commandHandler, scenes } from "./scenes";

const params = new URLSearchParams(location.search);
const scene = params.get("scene") ?? "home";
const theme = params.get("theme") ?? "dark";

// Skip onboarding overlay; preset the history filter to show varied statuses.
localStorage.setItem("sqz-onboarded", "1");
localStorage.setItem("sqz-theme", theme);
if (scene === "history") {
  localStorage.setItem(
    "sqz-history-filter",
    JSON.stringify(["done", "normalized", "failed", "skipped_no_gain", "skipped_already_efficient"]),
  );
}

mockWindows("main");
mockIPC((cmd, args) => commandHandler(cmd, args, scene), { shouldMockEvents: true });

// Render without StrictMode so engine-event listeners aren't double-mounted
// (which would double-count emitted events in the store).
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<App />);

// Exposed for the Playwright capture driver.
(window as { sqz?: unknown }).sqz = {
  ready: false,
  runScene: () => scenes[scene]?.(emit),
};

// Give the store's async event subscription time to register before scenes emit.
setTimeout(() => {
  (window as { sqz?: { ready: boolean } }).sqz!.ready = true;
}, 400);
