import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText, type PackDetail, type Snapshot } from "./lib/api";
import KeyboardPage from "./pages/KeyboardPage";
import PacksPage from "./pages/PacksPage";
import SettingsPage from "./pages/SettingsPage";
import PermissionGate from "./components/PermissionGate";
import { ToastHost, useToasts } from "./components/Toast";

type Page = "keyboard" | "packs" | "settings";

export default function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [page, setPage] = useState<Page>("keyboard");
  const [loadError, setLoadError] = useState<string | null>(null);
  const toasts = useToasts();

  const refresh = useCallback(async () => {
    try {
      setSnap(await api.getState());
      setLoadError(null);
    } catch (e) {
      setLoadError(errorText(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    const un = listen<boolean>("enabled-changed", (e) =>
      setSnap((s) => (s ? { ...s, settings: { ...s.settings, enabled: e.payload } } : s)),
    );
    return () => {
      un.then((f) => f());
    };
  }, [refresh]);

  // Only receive live key events while the window is visible.
  useEffect(() => {
    const sync = () => api.setKeyFeedback(document.visibilityState === "visible").catch(() => {});
    sync();
    document.addEventListener("visibilitychange", sync);
    window.addEventListener("focus", sync);
    return () => {
      document.removeEventListener("visibilitychange", sync);
      window.removeEventListener("focus", sync);
      api.setKeyFeedback(false).catch(() => {});
    };
  }, []);

  const setActive = useCallback((active: PackDetail | null) => {
    setSnap((s) => (s ? { ...s, active } : s));
  }, []);

  const volTimer = useRef<number | undefined>(undefined);
  const onVolume = (v: number) => {
    setSnap((s) => (s ? { ...s, settings: { ...s.settings, volume: v } } : s));
    window.clearTimeout(volTimer.current);
    volTimer.current = window.setTimeout(() => api.setVolume(v).catch(() => {}), 40);
  };

  const onToggle = async () => {
    if (!snap) return;
    try {
      await api.setEnabled(!snap.settings.enabled);
    } catch (e) {
      toasts.error(errorText(e));
    }
  };

  if (!snap) {
    return (
      <div className="boot">
        {loadError ? <p className="error-text">Keytone failed to start: {loadError}</p> : <div className="spinner" />}
      </div>
    );
  }

  const activeSummary = snap.packs.find((p) => p.id === snap.settings.active_pack);

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark" aria-hidden />
          Keytone
        </div>
        <nav>
          {(
            [
              ["keyboard", "Keyboard"],
              ["packs", "Sound packs"],
              ["settings", "Settings"],
            ] as [Page, string][]
          ).map(([id, label]) => (
            <button key={id} className={`nav-item ${page === id ? "active" : ""}`} onClick={() => setPage(id)}>
              {label}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
          <div className={`status-dot ${snap.hook.running && snap.settings.enabled ? "on" : ""}`} />
          {snap.hook.running ? (snap.settings.enabled ? "Listening" : "Paused") : "Not listening"}
        </div>
      </aside>

      <main className="main">
        <header className="topbar">
          <div className="topbar-pack">
            <span className="muted">Pack</span>
            <select
              value={snap.settings.active_pack ?? ""}
              onChange={async (e) => {
                try {
                  const d = await api.setActivePack(e.target.value);
                  setSnap((s) => (s ? { ...s, active: d, settings: { ...s.settings, active_pack: e.target.value } } : s));
                } catch (err) {
                  toasts.error(errorText(err));
                }
              }}
            >
              {snap.packs.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>
          <label className="volume">
            <span className="muted">Volume</span>
            <input type="range" min={0} max={1} step={0.01} value={snap.settings.volume} onChange={(e) => onVolume(Number(e.target.value))} />
          </label>
          <button
            className={`switch ${snap.settings.enabled ? "on" : ""}`}
            role="switch"
            aria-checked={snap.settings.enabled}
            onClick={onToggle}
            title="Turn key sounds on or off"
          >
            <span className="switch-knob" />
            <span className="switch-label">{snap.settings.enabled ? "Sounds on" : "Sounds off"}</span>
          </button>
        </header>

        {snap.hook.error_kind === "unsupported" && <div className="banner warn">{snap.hook.error}</div>}
        {snap.hook.error_kind === "other" && (
          <div className="banner warn">
            Couldn’t start listening to the keyboard: {snap.hook.error}{" "}
            <button className="link" onClick={() => api.retryHook().then(refresh)}>
              Try again
            </button>
          </div>
        )}
        {snap.engine.error && (
          <div className="banner warn">
            No sound output: {snap.engine.error}.{" "}
            <button className="link" onClick={() => setPage("settings")}>
              Choose an output device
            </button>
          </div>
        )}
        {snap.active_error && <div className="banner warn">{snap.active_error}</div>}

        <section className="content">
          {page === "keyboard" &&
            (snap.active ? (
              <KeyboardPage snap={snap} active={snap.active} setActive={setActive} toasts={toasts} />
            ) : (
              <div className="empty">
                <h2>No sound pack selected</h2>
                <p>Pick or create a pack to start mapping sounds to keys.</p>
                <button className="btn primary" onClick={() => setPage("packs")}>
                  Go to sound packs
                </button>
              </div>
            ))}
          {page === "packs" && <PacksPage snap={snap} refresh={refresh} setActive={setActive} toasts={toasts} activeName={activeSummary?.name} />}
          {page === "settings" && <SettingsPage snap={snap} setSnap={setSnap} refresh={refresh} toasts={toasts} />}
        </section>
      </main>

      <PermissionGate snap={snap} refresh={refresh} />
      <ToastHost toasts={toasts} />
    </div>
  );
}
