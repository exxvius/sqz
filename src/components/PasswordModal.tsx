import { useState, type FormEvent } from "react";

export type PasswordModalMode = "setup" | "unlock" | "change";

interface Props {
  mode: PasswordModalMode;
  /** Resolve to close; throw an Error to surface its message inline. */
  onSubmit: (values: {
    password?: string;
    oldPassword?: string;
    newPassword?: string;
  }) => Promise<void>;
  onClose: () => void;
}

const COPY: Record<
  PasswordModalMode,
  { title: string; note: string; submit: string }
> = {
  setup: {
    title: "Set a lock password",
    note: "You'll need this password to unlock the app. There's no recovery — if you forget it, the only way out is deleting lock.json from the app's data folder while sqz is closed.",
    submit: "Lock app",
  },
  unlock: {
    title: "Unlock the app",
    note: "Enter your password to reveal file names and re-enable controls and editing.",
    submit: "Unlock",
  },
  change: {
    title: "Change lock password",
    note: "Enter your current password, then a new one.",
    submit: "Change password",
  },
};

export function PasswordModal({ mode, onSubmit, onClose }: Props) {
  const copy = COPY[mode];
  const [oldPassword, setOldPassword] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const needsConfirm = mode === "setup" || mode === "change";

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (needsConfirm && password !== confirm) {
      setError("Passwords don't match.");
      return;
    }
    if ((mode === "setup" || mode === "change") && password.length === 0) {
      setError("Password can't be empty.");
      return;
    }
    setBusy(true);
    try {
      if (mode === "unlock") {
        await onSubmit({ password });
      } else if (mode === "setup") {
        await onSubmit({ password });
      } else {
        await onSubmit({ oldPassword, newPassword: password });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  };

  return (
    <div
      className="overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="pw-title"
    >
      <form className="card pw-modal" onSubmit={submit}>
        <h2 id="pw-title">{copy.title}</h2>
        <p className="muted">{copy.note}</p>

        {mode === "change" && (
          <label className="pw-field">
            <span>Current password</span>
            <input
              type="password"
              autoFocus
              value={oldPassword}
              onChange={(e) => setOldPassword(e.target.value)}
            />
          </label>
        )}

        <label className="pw-field">
          <span>{mode === "unlock" ? "Password" : "New password"}</span>
          <input
            type="password"
            autoFocus={mode !== "change"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>

        {needsConfirm && (
          <label className="pw-field">
            <span>Confirm password</span>
            <input
              type="password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
          </label>
        )}

        {error && <div className="err-box">{error}</div>}

        <div className="pw-actions">
          <button
            type="button"
            className="btn ghost"
            onClick={onClose}
            disabled={busy}
          >
            Cancel
          </button>
          <button type="submit" className="btn primary" disabled={busy}>
            {busy ? "Working…" : copy.submit}
          </button>
        </div>
      </form>
    </div>
  );
}
