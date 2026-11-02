import { useEffect, useState } from "react";
import { api, type Snapshot } from "../lib/api";

/**
 * macOS only: Keytone needs the "Input Monitoring" permission to hear key
 * presses in other apps. Walks the user through granting it and starts
 * listening as soon as it is granted.
 */
export default function PermissionGate({ snap, refresh }: { snap: Snapshot; refresh: () => void }) {
  const needs = snap.platform === "macos" && !snap.hook.running && snap.hook.error_kind === "permission";
  const [tries, setTries] = useState(0);

  useEffect(() => {
    if (!needs) return;
    const t = window.setInterval(async () => {
      const st = await api.retryHook().catch(() => null);
      setTries((n) => n + 1);
      if (st?.running) refresh();
    }, 2000);
    return () => window.clearInterval(t);
  }, [needs, refresh]);

  if (!needs) return null;

  return (
    <div className="modal-backdrop">
      <div className="modal perm" role="dialog" aria-label="Permission needed">
        <h2>Let Keytone hear your keyboard</h2>
        <p>
          To play a sound when you type in other apps, macOS needs you to allow <b>Input Monitoring</b> for Keytone.
        </p>
        <ol className="steps">
          <li>
            <button className="btn primary" onClick={() => api.requestPermission().then(refresh)}>
              Ask for permission
            </button>
          </li>
          <li>
            If nothing appears,{" "}
            <button className="link" onClick={() => api.openPermissionSettings()}>
              open Privacy &amp; Security → Input Monitoring
            </button>{" "}
            and switch Keytone on.
          </li>
          <li>Come back here — Keytone starts automatically once it’s allowed.</li>
        </ol>
        <div className="privacy-note">
          Keytone only sees <i>which</i> key moved, never what you type. Nothing is stored or sent anywhere.
        </div>
        {tries > 5 && (
          <p className="muted small">
            Already switched on? macOS sometimes needs a restart of the app.{" "}
            <button className="link" onClick={() => api.restartApp()}>
              Restart Keytone
            </button>
          </p>
        )}
      </div>
    </div>
  );
}
