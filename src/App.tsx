import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DashboardView } from "./views/DashboardView";
import { HistoryView } from "./views/HistoryView";
import { HomeView } from "./views/HomeView";
import { Onboarding } from "./views/Onboarding";
import { SettingsView } from "./views/SettingsView";
import { api } from "./lib/api";
import { defaultConfig, fromPersisted, persistable } from "./lib/config";
import { HistoryIcon, HomeIcon, LiveIcon, Logo, SettingsIcon } from "./components/icons";
import { StoreProvider, useStore } from "./lib/store";
import { useTheme } from "./lib/theme";
import { useAccent } from "./lib/accent";
import { initCursorFx } from "./lib/cursor";
import type { FfStatus, RunConfig } from "./lib/types";
import type { ComponentType } from "react";

type View = "home" | "dashboard" | "history" | "settings";

const NAV: { id: View; label: string; icon: ComponentType<{ size?: number }> }[] = [
  { id: "home", label: "Home", icon: HomeIcon },
  { id: "dashboard", label: "Live", icon: LiveIcon },
  { id: "history", label: "History", icon: HistoryIcon },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

function Shell() {
  const store = useStore();
  const [theme, toggleTheme] = useTheme();
  const [accent, setAccent] = useAccent();
  const [view, setView] = useState<View>("home");
  const [config, setConfig] = useState<RunConfig>(defaultConfig);
  const [showOnboarding, setShowOnboarding] = useState(
    () => !localStorage.getItem("sqz-onboarded"),
  );

  // Cursor-driven spotlight/border-glow on cards + background parallax.
  useEffect(() => initCursorFx(), []);

  const loaded = useRef(false);
  const [ff, setFf] = useState<FfStatus | null>(null);
  const refreshFf = useCallback(() => {
    api.ffmpegStatus().then(setFf);
  }, []);

  useEffect(() => {
    refreshFf();
    // Restore all persisted settings (everything except the input list).
    api.getSettings().then((saved) => {
      setConfig((c) => ({ ...fromPersisted(saved), inputs: c.inputs }));
      loaded.current = true;
    });
  }, [refreshFf]);

  // Persist settings (debounced) whenever the config changes, once loaded.
  useEffect(() => {
    if (!loaded.current) return;
    const id = setTimeout(() => {
      api.saveSettings(persistable(config)).catch(() => {});
    }, 400);
    return () => clearTimeout(id);
  }, [config]);

  const dismissOnboarding = () => {
    localStorage.setItem("sqz-onboarded", "1");
    setShowOnboarding(false);
  };

  const body = useMemo(() => {
    switch (view) {
      case "home":
        return (
          <HomeView
            config={config}
            setConfig={setConfig}
            goDashboard={() => setView("dashboard")}
            ff={ff}
            refreshFf={refreshFf}
            goSettings={() => setView("settings")}
          />
        );
      case "dashboard":
        return <DashboardView />;
      case "history":
        return <HistoryView />;
      case "settings":
        return (
          <SettingsView
            theme={theme}
            toggleTheme={toggleTheme}
            accent={accent}
            setAccent={setAccent}
            ff={ff}
            refreshFf={refreshFf}
          />
        );
    }
  }, [view, config, theme, toggleTheme, accent, setAccent, ff, refreshFf]);

  return (
    <div className="app">
      <div className="lava" aria-hidden="true">
        <i />
        <i />
        <i />
        <i />
      </div>

      <aside className="sidebar">
        <div className="brand">
          <Logo size={30} />
          <span className="wordmark">sqz</span>
        </div>

        <nav aria-label="Main">
          {NAV.map((n) => {
            const Icon = n.icon;
            const processing = n.id === "dashboard" && store.running;
            return (
              <button
                key={n.id}
                className={`nav-item${processing ? " processing" : ""}`}
                aria-current={view === n.id}
                onClick={() => setView(n.id)}
              >
                <span className="glyph">
                  <Icon size={18} />
                </span>
                <span className="grow">{n.label}</span>
              </button>
            );
          })}
        </nav>

        <div className="sidebar-foot">
          <button className="theme-toggle" onClick={toggleTheme}>
            {theme === "dark" ? "🌙 Dark" : "☀️ Light"}
          </button>
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
