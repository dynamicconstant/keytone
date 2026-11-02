import { useEffect, useState } from "react";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { api, errorText, type PackDetail, type Snapshot } from "../lib/api";
import type { Toasts } from "../components/Toast";

interface Props {
  snap: Snapshot;
  refresh: () => Promise<void>;
  setActive: (d: PackDetail | null) => void;
  toasts: Toasts;
  activeName?: string;
}

function hue(id: string) {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}

export default function PacksPage({ snap, refresh, setActive, toasts }: Props) {
  const [busy, setBusy] = useState(false);

  const run = async (fn: () => Promise<unknown>, ok?: string) => {
    setBusy(true);
    try {
      await fn();
      await refresh();
      if (ok) toasts.info(ok);
    } catch (e) {
      toasts.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const use = (id: string) =>
    run(async () => {
      setActive(await api.setActivePack(id));
    });

  const importArchive = async () => {
    const p = await open({ filters: [{ name: "Sound pack", extensions: ["keytone", "zip"] }] });
    if (typeof p === "string") run(async () => use(await api.importPack(p)), "Pack imported");
  };
  const importFolder = async () => {
    const p = await open({ directory: true });
    if (typeof p === "string") run(async () => use(await api.importPack(p)), "Pack imported");
  };

  return (
    <div className="packs-page">
      <div className="page-head">
        <h1>Sound packs</h1>
        <div className="row">
          <button
            className="btn primary"
            disabled={busy}
            onClick={() => run(async () => use(await api.createPack("My sounds")), "Created an empty pack — add sounds on the Keyboard page")}
          >
            New pack
          </button>
          <button className="btn" disabled={busy} onClick={importArchive}>
            Import .keytone / .zip…
          </button>
          <button className="btn" disabled={busy} onClick={importFolder} title="Keytone or Mechvibes pack folder">
            Import folder…
          </button>
        </div>
      </div>
      <p className="muted">
        Folders from <b>Mechvibes</b> (with a <code>config.json</code>) are converted automatically.
      </p>

      <div className="pack-grid">
        {snap.packs.map((p) => {
          const isActive = p.id === snap.settings.active_pack;
          return (
            <div key={p.id} className={`pack-card ${isActive ? "active" : ""}`}>
              <button className="pack-cover" style={{ ["--h" as string]: hue(p.id) }} onClick={() => use(p.id)} disabled={busy}>
                <span>{p.name.slice(0, 2)}</span>
              </button>
              <div className="pack-body">
                <div className="pack-title">
                  {p.name}
                  {isActive && <span className="tag accent">Active</span>}
                </div>
                <div className="muted small clamp">{p.description || (p.author ? `by ${p.author}` : " ")}</div>
                <div className="pack-actions">
                  {!isActive && (
                    <button className="btn small" disabled={busy} onClick={() => use(p.id)}>
                      Use
                    </button>
                  )}
                  <button
                    className="btn small ghost"
                    disabled={busy}
                    onClick={() => run(async () => use(await api.duplicatePack(p.id, `${p.name} copy`)), "Duplicated")}
                  >
                    Duplicate
                  </button>
                  <button
                    className="btn small ghost"
                    disabled={busy}
                    onClick={async () => {
                      const dest = await save({ defaultPath: `${p.id}.keytone`, filters: [{ name: "Keytone pack", extensions: ["keytone"] }] });
                      if (dest) run(() => api.exportPack(p.id, dest), "Exported");
                    }}
                  >
                    Export
                  </button>
                  <button
                    className="btn small ghost danger"
                    disabled={busy || snap.packs.length <= 1}
                    onClick={async () => {
                      if (await ask(`Delete “${p.name}”? This removes its sounds too.`, { title: "Delete pack", kind: "warning" }))
                        run(() => api.deletePack(p.id), "Deleted");
                    }}
                  >
                    Delete
                  </button>
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {snap.active && <PackSettings detail={snap.active} setActive={setActive} refresh={refresh} toasts={toasts} />}
    </div>
  );
}

function PackSettings({ detail, setActive, refresh, toasts }: { detail: PackDetail; setActive: (d: PackDetail | null) => void; refresh: () => Promise<void>; toasts: Toasts }) {
  const m = detail.meta;
  const [name, setName] = useState(m.name);
  const [author, setAuthor] = useState(m.author);
  const [description, setDescription] = useState(m.description);
  const [volume, setVolume] = useState(m.volume);
  const [pitch, setPitch] = useState(m.random.pitch_cents);
  const [gain, setGain] = useState(m.random.gain_db);

  useEffect(() => {
    setName(m.name);
    setAuthor(m.author);
    setDescription(m.description);
    setVolume(m.volume);
    setPitch(m.random.pitch_cents);
    setGain(m.random.gain_db);
  }, [detail.id, m]);

  const commit = async (patch: Parameters<typeof api.updatePackMeta>[0], reloadList = false) => {
    try {
      setActive(await api.updatePackMeta(patch));
      if (reloadList) await refresh();
    } catch (e) {
      toasts.error(errorText(e));
    }
  };

  return (
    <section className="card">
      <h2>Active pack settings</h2>
      <div className="form-grid">
        <label className="field">
          <span>Name</span>
          <input value={name} onChange={(e) => setName(e.target.value)} onBlur={() => name !== m.name && commit({ name }, true)} />
        </label>
        <label className="field">
          <span>Author</span>
          <input value={author} onChange={(e) => setAuthor(e.target.value)} onBlur={() => author !== m.author && commit({ author }, true)} />
        </label>
        <label className="field wide">
          <span>Description</span>
          <input value={description} onChange={(e) => setDescription(e.target.value)} onBlur={() => description !== m.description && commit({ description }, true)} />
        </label>
        <label className="field">
          <span>Pack loudness · {Math.round(volume * 100)}%</span>
          <input type="range" min={0} max={2} step={0.05} value={volume} onChange={(e) => setVolume(Number(e.target.value))} onPointerUp={() => commit({ volume })} onKeyUp={() => commit({ volume })} />
        </label>
        <label className="field">
          <span>Pitch variation · ±{Math.round(pitch)} cents</span>
          <input type="range" min={0} max={150} step={5} value={pitch} onChange={(e) => setPitch(Number(e.target.value))} onPointerUp={() => commit({ random: { pitch_cents: pitch, gain_db: gain } })} onKeyUp={() => commit({ random: { pitch_cents: pitch, gain_db: gain } })} />
        </label>
        <label className="field">
          <span>Loudness variation · ±{gain.toFixed(1)} dB</span>
          <input type="range" min={0} max={6} step={0.5} value={gain} onChange={(e) => setGain(Number(e.target.value))} onPointerUp={() => commit({ random: { pitch_cents: pitch, gain_db: gain } })} onKeyUp={() => commit({ random: { pitch_cents: pitch, gain_db: gain } })} />
        </label>
        <label className="field check">
          <input type="checkbox" checked={m.trim_silence} onChange={(e) => commit({ trim_silence: e.target.checked })} />
          <span>Trim silence at the start of sounds (makes them feel instant)</span>
        </label>
      </div>
      <p className="muted small">Small random pitch and loudness changes on every press make typing sound natural instead of robotic.</p>
    </section>
  );
}
