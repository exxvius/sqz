// Lock context: exposes the current locked state + password-gated actions to the
// whole app, and provides `maskName`/`maskPath` already bound to the locked flag
// so components don't have to thread it through by hand. When locked, the app
// masks personal info and becomes read-only (no run control, settings, or edits).

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { api } from "./api";
import { maskName as rawMaskName, maskPath as rawMaskPath } from "./mask";

interface LockValue {
  /** A password has been set up at least once. */
  configured: boolean;
  /** The app is currently locked (masked + read-only). */
  locked: boolean;
  refresh: () => Promise<void>;
  setup: (password: string) => Promise<void>;
  lock: () => Promise<void>;
  unlock: (password: string) => Promise<void>;
  changePassword: (oldPassword: string, newPassword: string) => Promise<void>;
  /** Returns the name when unlocked, a redacted placeholder when locked. */
  maskName: (name: string) => string;
  /** Returns the path when unlocked, a redacted placeholder when locked. */
  maskPath: (path: string) => string;
}

const LockContext = createContext<LockValue | null>(null);

export function LockProvider({ children }: { children: ReactNode }) {
  const [configured, setConfigured] = useState(false);
  const [locked, setLocked] = useState(false);

  const refresh = useCallback(async () => {
    const s = await api.lockStatus();
    setConfigured(s.configured);
    setLocked(s.locked);
  }, []);

  useEffect(() => {
    refresh().catch(() => {});
  }, [refresh]);

  const value = useMemo<LockValue>(
    () => ({
      configured,
      locked,
      refresh,
      setup: async (password) => {
        await api.lockSetup(password);
        await refresh();
      },
      lock: async () => {
        await api.lockApp();
        await refresh();
      },
      unlock: async (password) => {
        await api.unlockApp(password);
        await refresh();
      },
      changePassword: async (oldPassword, newPassword) => {
        await api.lockChangePassword(oldPassword, newPassword);
        await refresh();
      },
      maskName: (name) => (locked ? rawMaskName(name) : name),
      maskPath: (path) => (locked ? rawMaskPath(path) : path),
    }),
    [configured, locked, refresh],
  );

  return <LockContext.Provider value={value}>{children}</LockContext.Provider>;
}

export function useLock(): LockValue {
  const ctx = useContext(LockContext);
  if (!ctx) throw new Error("useLock must be used within LockProvider");
  return ctx;
}
