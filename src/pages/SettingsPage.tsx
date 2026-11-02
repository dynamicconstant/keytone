import { useEffect, useState } from "react";
import { api, errorText, type SettingsPatch, type Snapshot } from "../lib/api";
import { acceleratorFromEvent, prettyAccelerator } from "../lib/keys";
import type { Toasts } from "../components/Toast";

interface Props {
  snap: Snapshot;
  setSnap: (s: Snapshot) => void;
  refresh: () => Promise<void>;
  toasts: Toasts;
}

const BUFFERS: (number | null)[] = [128, 256, 512, 1024, null];

export default function SettingsPage({ snap, setSnap, toasts }: Props) {
  const s = snap.settings;
  const [devices, setDevices] = useState<string[]>([]);
  const [capturing, setCapturing] = useState(false);

  useEffect(() => {
    api.listDevices().then(setDevices).catch(() => {});
  }, []);

  const patch = async (p: SettingsPatch) => {
    try {
      setSnap(await api.updateSettings(p));
    } catch (e) {
      toasts.error(errorText(e));
    }
  };

  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") return setCapturing(false);
      const acc = acceleratorFromEvent(e, snap.platform);
      if (acc) {
        setCapturing(false);
        patch({ toggle_hotkey: acc });
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [capturing]);

  const e = snap.engine;

  return (
    <div className="settings-page">
      <h1>Settings</h1>

      <section className="card">
        <h2>General</h2>
        <div className="setting">
          <div>
            <div className="setting-title">On/off shortcut</div>
            <div className="muted small">Works from any app.</div>
          </div>
          <div className="row">
            <kbd className={capturing ? "capturing" : ""}>{capturing ? "Press a shortcut…" : prettyAccelerator(s.toggle_hotkey, snap.platform)}</kbd>
            <button className="btn small" onClick={() => setCapturing((c) => !c)}>
              {capturing ? "Cancel" : "Change"}
            </button>
            {s.toggle_hotkey && (
              <button className="btn small ghost" onClick={() => patch({ toggle_hotkey: null })}>
                Clear
              </button>
            )}
          </div>
        </div>
        <div className="setting">
          <div>
            <div className="setting-title">Launch at login</div>
            <div className="muted small">Starts quietly in the {snap.platform === "macos" ? "menu bar" : "system tray"}.</div>
          </div>
          <input type="checkbox" className="toggle" checked={s.launch_at_login} onChange={(ev) => patch({ launch_at_login: ev.target.checked })} />
        </div>
      </section>

      <section className="card">
        <h2>Audio</h2>
        <div className="setting">
          <div>
            <div className="setting-title">Output device</div>
            <div className="muted small">“System default” follows your headphones when you plug them in.</div>
          </div>
          <select value={s.output_device ?? ""} onChange={(ev) => patch({ output_device: ev.target.value || null })}>
            <option value="">System default</option>
            {devices.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </div>
        <div className="setting">
          <div>
            <div className="setting-title">Buffer size</div>
            <div className="muted small">Smaller = snappier. Go bigger if you hear crackles.</div>
          </div>
          <select value={s.buffer_frames ?? ""} onChange={(ev) => patch({ buffer_frames: ev.target.value ? Number(ev.target.value) : null })}>
            {BUFFERS.map((b) => (
              <option key={b ?? "default"} value={b ?? ""}>
                {b ? `${b} frames` : "Driver default"}
              </option>
            ))}
          </select>
        </div>
        <div className="engine-info muted small">
          {e.error ? (
            <span className="error-text">{e.error}</span>
          ) : (
            <>
              Playing through <b>{e.device}</b> · {e.sample_rate / 1000} kHz · {e.channels} ch
              {e.latency_ms != null && <> · buffer ≈ {e.latency_ms.toFixed(1)} ms</>}
              {e.note && <div>{e.note}.</div>}
            </>
          )}
        </div>
      </section>

      <section className="card">
        <h2>Privacy</h2>
        <ul className="privacy">
          <li>Keytone sees only <b>which physical key</b> went down or up — never the characters you type.</li>
          <li>Nothing about your typing is logged, stored, or sent anywhere. Keytone makes no network requests.</li>
          <li>Key events reach this window only while it’s open, to light up the on-screen keyboard.</li>
        </ul>
        <div className="muted small">
          Keyboard listener: {snap.hook.running ? "running" : `not running${snap.hook.error ? ` — ${snap.hook.error}` : ""}`}
          {snap.platform === "macos" && <> · Input Monitoring: {snap.hook.permission || "unknown"}</>}
        </div>
      </section>

      <p className="muted small">Keytone {snap.version}</p>
    </div>
  );
}
