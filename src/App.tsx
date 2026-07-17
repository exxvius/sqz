import { useEffect, useMemo, useState } from "react";
import { DashboardView } from "./views/DashboardView";
import { HistoryView } from "./views/HistoryView";
import { HomeView } from "./views/HomeView";
import { Onboarding } from "./views/Onboarding";
import { SettingsView } from "./views/SettingsView";
import { api } from "./lib/api";
import { applyDefaults, defaultConfig, type PersistedDefaults } from "./lib/config";
import { StoreProvider, useStore } from "./lib/store";
import { useTheme } from "./lib/theme";
import type { FfStatus, RunConfig } from "./lib/types";

type View = "home" | "dashboard" | "history" | "settings";

const NAV: { id: View; label: string; glyph: string }[] = [
  { id: "home", label: "Home", glyph: "▚" },
  { id: "dashboard", label: "Live", glyph: "◈" },
  { id: "history", label: "History", glyph: "≣" },
  { id: "settings", label: "Settings", glyph: "⚙" },
];

function Shell() {
  const store = useStore();
  const [theme, toggleTheme] = useTheme();
  const [view, setView] = useState<View>("home");
  const [config, setConfig] = useState<RunConfig>(defaultConfig);
  const [ff, setFf] = useState<FfStatus | null>(null);
  const [showOnboarding, setShowOnboarding] = useState(
    () => !localStorage.getItem("sqz-onboarded"),
  );

  useEffect(() => {
    api.ffmpegStatus().then(setFf);
    // Seed the config form with the user's saved defaults.
    api.getSettings().then((saved) => {
      setConfig((c) => applyDefaults(c, saved as Partial<PersistedDefaults>));
    });
  }, []);

  const dismissOnboarding = () => {
    localStorage.setItem("sqz-onboarded", "1");
    setShowOnboarding(false);
  };

  const body = useMemo(() => {
    switch (view) {
      case "home":
        return (
          <HomeView config={config} setConfig={setConfig} goDashboard={() => setView("dashboard")} />
        );
      case "dashboard":
        return <DashboardView />;
      case "history":
        return <HistoryView />;
      case "settings":
        return <SettingsView theme={theme} toggleTheme={toggleTheme} />;
    }
  }, [view, config, theme, toggleTheme]);

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <h1>sqz</h1>
          <span className="dot" />
        </div>

        <nav aria-label="Main">
          {NAV.map((n) => (
            <button
              key={n.id}
              className="nav-item"
              aria-current={view === n.id}
              onClick={() => setView(n.id)}
            >
              <span className="glyph">{n.glyph}</span>
              <span className="grow">{n.label}</span>
              {n.id === "dashboard" && store.running && (
                <span className="status-dot ok" title="Run in progress" />
              )}
            </button>
          ))}
        </nav>

        <div className="sidebar-foot">
          <button className="theme-toggle" onClick={toggleTheme}>
            {theme === "dark" ? "🌙 Dark" : "☀️ Light"}
          </button>
          <div className="ff-badge">
            <span className={`status-dot ${ff?.present ? "ok" : "bad"}`} />
            {ff?.present ? "FFmpeg bundled" : "FFmpeg via PATH"}
          </div>
        </div>
      </aside>

      <main className="main">{body}</main>

      {showOnboarding && <Onboarding onClose={dismissOnboarding} />}
    </div>
  );
}

export default function App() {
  return (
    <StoreProvider>
      <Shell />
    </StoreProvider>
  );
}
