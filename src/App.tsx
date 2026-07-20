import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DashboardView } from "./views/DashboardView";
import { HistoryView } from "./views/HistoryView";
import { HomeView } from "./views/HomeView";
import { LibraryView } from "./views/LibraryView";
import { Onboarding } from "./views/Onboarding";
import { SettingsView } from "./views/SettingsView";
import { api } from "./lib/api";
import { defaultConfig, fromPersisted, persistable } from "./lib/config";
import {
  HistoryIcon,
  HomeIcon,
  LibraryIcon,
  LiveIcon,
  LockIcon,
  Logo,
  MoonIcon,
  SettingsIcon,
  SunIcon,
  UnlockIcon,
} from "./components/icons";
import { PasswordModal, type PasswordModalMode } from "./components/PasswordModal";
import { CloseWarningModal } from "./components/CloseWarningModal";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StoreProvider, useStore } from "./lib/store";
import { ProbeProvider, useProbes } from "./lib/probes";
import { LockProvider, useLock } from "./lib/lock";
import { useTheme } from "./lib/theme";
import { useAccent } from "./lib/accent";
import { useCloseBehavior } from "./lib/closeBehavior";
import { initCursorFx } from "./lib/cursor";
import type { FfStatus, RunConfig } from "./lib/types";
import type { ComponentType } from "react";

type View = "home" | "dashboard" | "library" | "history" | "settings";

const NAV: { id: View; label: string; icon: ComponentType<{ size?: number }> }[] = [
  { id: "home", label: "Home", icon: HomeIcon },
  { id: "dashboard", label: "Live", icon: LiveIcon },
  { id: "library", label: "Library", icon: LibraryIcon },
  { id: "history", label: "History", icon: HistoryIcon },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

function Shell() {
  const store = useStore();
  const lock = useLock();
  const [theme, toggleTheme] = useTheme();
  const [accent, setAccent] = useAccent();
  const [closeBehavior, setCloseBehavior] = useCloseBehavior();
  const [view, setView] = useState<View>("home");
  const [pwModal, setPwModal] = useState<PasswordModalMode | null>(null);
  const [closeWarn, setCloseWarn] = useState(false);
  const [config, setConfig] = useState<RunConfig>(defaultConfig);
  const [showOnboarding, setShowOnboarding] = useState(
    () => !localStorage.getItem("sqz-onboarded"),
  );

  // Cursor-driven spotlight/border-glow on cards + background parallax.
  useEffect(() => initCursorFx(), []);

  // The window close handler is registered once; read live values via refs.
  const behaviorRef = useRef(closeBehavior);
  const runningRef = useRef(store.running);
  useEffect(() => {
    behaviorRef.current = closeBehavior;
  }, [closeBehavior]);
  useEffect(() => {
    runningRef.current = store.running;
  }, [store.running]);

  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    win
      .onCloseRequested(async (event) => {
        // Always take control of the close, then act explicitly. (Letting the
        // default proceed relies on the window's internal destroy(), which needs
        // a permission we don't grant — so we quit via the backend instead.)
        event.preventDefault();
        if (behaviorRef.current === "tray") {
          await win.hide();
        } else if (runningRef.current) {
          // Quitting mid-run: confirm first, offering the tray as an alternative.
          setCloseWarn(true);
        } else {
          await api.quitApp();
        }
      })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  }, []);

  const probes = useProbes();
  const loaded = useRef(false);
  const [ff, setFf] = useState<FfStatus | null>(null);
  const refreshFfStatus = useCallback(() => {
    api.ffmpegStatus().then(setFf);
  }, []);
  // An FFmpeg change (download / custom path / clear) invalidates the cached
  // hardware + environment probes, so re-run them too.
  const refreshFf = useCallback(() => {
    refreshFfStatus();
    probes.redetect();
    probes.recheckEnv();
  }, [refreshFfStatus, probes]);

  useEffect(() => {
    refreshFfStatus();
    // Restore all persisted settings (everything except the input list).
    api.getSettings().then((saved) => {
      setConfig((c) => ({ ...fromPersisted(saved), inputs: c.inputs }));
      loaded.current = true;
    });
  }, [refreshFfStatus]);

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

  // Sidebar lock toggle: first time sets a password, later locks without a
  // prompt and unlocks behind the password.
  const onToggleLock = () => {
    if (!lock.configured) setPwModal("setup");
    else if (!lock.locked) lock.lock().catch(() => {});
    else setPwModal("unlock");
  };

  const handlePwSubmit = async (values: {
    password?: string;
    oldPassword?: string;
    newPassword?: string;
  }) => {
    if (pwModal === "setup") {
      await lock.setup(values.password ?? "");
      await lock.lock();
    } else if (pwModal === "unlock") {
      await lock.unlock(values.password ?? "");
    } else if (pwModal === "change") {
      await lock.changePassword(values.oldPassword ?? "", values.newPassword ?? "");
    }
    setPwModal(null);
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
      case "library":
        return <LibraryView config={config} />;
      case "history":
        return <HistoryView />;
      case "settings":
        return (
          <SettingsView
            theme={theme}
            toggleTheme={toggleTheme}
            accent={accent}
            setAccent={setAccent}
            closeBehavior={closeBehavior}
            setCloseBehavior={setCloseBehavior}
            ff={ff}
            refreshFf={refreshFf}
          />
        );
    }
  }, [view, config, theme, toggleTheme, accent, setAccent, closeBehavior, setCloseBehavior, ff, refreshFf]);

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
          <button
            className={`foot-btn${lock.locked ? " on" : ""}`}
            onClick={onToggleLock}
            aria-pressed={lock.locked}
            title={
              lock.locked
                ? "Locked — click to unlock (password required)"
                : "Lock the app: hide personal info and make it read-only"
            }
          >
            {lock.locked ? <LockIcon size={15} /> : <UnlockIcon size={15} />}
            <span>{lock.locked ? "Locked" : "Lock"}</span>
          </button>
          <button className="foot-btn" onClick={toggleTheme} disabled={lock.locked}>
            {theme === "dark" ? <MoonIcon size={15} /> : <SunIcon size={15} />}
            <span>{theme === "dark" ? "Dark" : "Light"}</span>
          </button>
        </div>
      </aside>

      <main className="main">{body}</main>

      {showOnboarding && <Onboarding onClose={dismissOnboarding} />}
      {pwModal && (
        <PasswordModal
          mode={pwModal}
          onSubmit={handlePwSubmit}
          onClose={() => setPwModal(null)}
        />
      )}
      {closeWarn && (
        <CloseWarningModal
          onQuit={() => {
            setCloseWarn(false);
            api.quitApp();
          }}
          onMinimize={async () => {
            setCloseWarn(false);
            await getCurrentWindow().hide();
          }}
          onCancel={() => setCloseWarn(false)}
        />
      )}
    </div>
  );
}

export default function App() {
  return (
    <StoreProvider>
      <LockProvider>
        <ProbeProvider>
          <Shell />
        </ProbeProvider>
      </LockProvider>
    </StoreProvider>
  );
}
