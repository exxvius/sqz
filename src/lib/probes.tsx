// Cached hardware/environment probes. Detecting encoders and reading the
// environment are one-time, machine-level facts — not per-view work. Probing
// them once at app root (instead of on every Home/Settings mount) means
// switching tabs no longer re-runs a real test-encode probe each time.

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
import type { Detection, EnvInfo } from "./types";

interface ProbeValue {
  detection: Detection | null;
  detecting: boolean;
  detectFailed: boolean;
  /** Re-run encoder detection (after an FFmpeg change, or a manual retry). */
  redetect: () => void;
  env: EnvInfo | null;
  envLoading: boolean;
  /** Re-read the environment (after an FFmpeg change, or a manual re-check). */
  recheckEnv: () => void;
}

const ProbeContext = createContext<ProbeValue | null>(null);

export function ProbeProvider({ children }: { children: ReactNode }) {
  const [detection, setDetection] = useState<Detection | null>(null);
  // Start "busy": both probes fire once on mount, and this avoids a first-frame
  // flash of empty/no-hardware content before the initial probe resolves.
  const [detecting, setDetecting] = useState(true);
  const [detectFailed, setDetectFailed] = useState(false);
  const [env, setEnv] = useState<EnvInfo | null>(null);
  const [envLoading, setEnvLoading] = useState(true);

  const redetect = useCallback(() => {
    setDetecting(true);
    setDetectFailed(false);
    api
      .detectEncoders()
      .then(setDetection)
      .catch(() => setDetectFailed(true))
      .finally(() => setDetecting(false));
  }, []);

  const recheckEnv = useCallback(() => {
    setEnvLoading(true);
    api
      .environment()
      .then(setEnv)
      .catch(() => setEnv(null))
      .finally(() => setEnvLoading(false));
  }, []);

  // Probe once at startup; every consumer reuses the cached result thereafter.
  useEffect(() => {
    redetect();
    recheckEnv();
  }, [redetect, recheckEnv]);

  const value = useMemo<ProbeValue>(
    () => ({ detection, detecting, detectFailed, redetect, env, envLoading, recheckEnv }),
    [detection, detecting, detectFailed, redetect, env, envLoading, recheckEnv],
  );

  return <ProbeContext.Provider value={value}>{children}</ProbeContext.Provider>;
}

export function useProbes(): ProbeValue {
  const ctx = useContext(ProbeContext);
  if (!ctx) throw new Error("useProbes must be used within ProbeProvider");
  return ctx;
}
