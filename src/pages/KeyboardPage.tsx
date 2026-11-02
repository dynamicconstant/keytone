import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { api, errorText, type Dir, type PackDetail, type Snapshot, type Target } from "../lib/api";
import { codeLabel, GROUPS, groupLabel, groupOf, layoutCodes, refLabel, resolve, slotFor } from "../lib/keys";
import Keyboard from "../components/Keyboard";
import RecorderModal from "../components/RecorderModal";
import type { Toasts } from "../components/Toast";

interface Props {
  snap: Snapshot;
  active: PackDetail;
  setActive: (d: PackDetail | null) => void;
  toasts: Toasts;
}

type Selection = { kind: "keys"; codes: string[] } | { kind: "group"; name: string } | { kind: "default" };

const AUDIO_EXT = ["wav", "mp3", "ogg", "oga", "flac", "m4a", "aac"];

export default function KeyboardPage({ snap, active, setActive, toasts }: Props) {
  const [pressed, setPressed] = useState<Set<string>>(new Set());
  const [sel, setSel] = useState<Selection>({ kind: "keys", codes: ["KeyA"] });
  const [recordFor, setRecordFor] = useState<Dir | null>(null);
  const [busy, setBusy] = useState(false);
  const meta = active.meta;

  // Live key highlight.
  useEffect(() => {
    const un = listen<{ code: string; down: boolean }>("key", (e) => {
      setPressed((prev) => {
        const next = new Set(prev);
        if (e.payload.down) next.add(e.payload.code);
        else next.delete(e.payload.code);
        return next;
      });
    });
    const clear = () => setPressed(new Set());
    window.addEventListener("blur", clear);
    return () => {
      un.then((f) => f());
      window.removeEventListener("blur", clear);
    };
  }, []);

  const targets: Target[] = useMemo(() => {
    if (sel.kind === "keys") return sel.codes.map((c) => ({ kind: "key", name: c }) as Target);
    if (sel.kind === "group") return [{ kind: "group", name: sel.name }];
    return [{ kind: "default" }];
  }, [sel]);

  const customized = useMemo(() => new Set(Object.keys(meta.keys)), [meta.keys]);
  const selectedKeys = useMemo(() => new Set(sel.kind === "keys" ? sel.codes : []), [sel]);
  const highlighted = useMemo(() => {
    if (sel.kind !== "group") return new Set<string>();
    return new Set(layoutCodes(snap.platform).filter((c) => groupOf(c) === sel.name));
  }, [sel, snap.platform]);

  const onKeyClick = (code: string, e: React.MouseEvent) => {
    if ((e.shiftKey || e.metaKey || e.ctrlKey) && sel.kind === "keys") {
      const set = new Set(sel.codes);
      if (set.has(code) && set.size > 1) set.delete(code);
      else set.add(code);
      setSel({ kind: "keys", codes: [...set] });
    } else {
      setSel({ kind: "keys", codes: [code] });
      api.previewKey(code).catch(() => {});
    }
  };

  const apply = async (dir: Dir, refs: string[] | null) => {
    setBusy(true);
    try {
      setActive(await api.setMapping(targets, dir, refs));
    } catch (e) {
      toasts.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const importFile = async (dir: Dir, current: string[]) => {
    const picked = await open({ multiple: true, filters: [{ name: "Audio", extensions: AUDIO_EXT }] });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    const added: string[] = [];
    for (const p of paths) {
      try {
        added.push(await api.importSound(p));
      } catch (e) {
        toasts.error(errorText(e));
      }
    }
    if (added.length) {
      await apply(dir, [...current, ...added]);
      toasts.info(`Added ${added.length} sound${added.length > 1 ? "s" : ""}`);
    }
  };

  const title =
    sel.kind === "keys"
      ? sel.codes.length === 1
        ? codeLabel(sel.codes[0])
        : `${sel.codes.length} keys`
      : sel.kind === "group"
        ? groupLabel(sel.name)
        : "Every key (pack default)";

  const choices = [
    ...active.files,
    ...Object.keys(meta.sprite?.segments ?? {}).map((s) => `#${s}`),
  ];

  return (
    <div className="kb-page">
      <div className="target-bar">
        <button className={`chip ${sel.kind === "default" ? "active" : ""}`} onClick={() => setSel({ kind: "default" })}>
          All keys
        </button>
        {GROUPS.map((g) => (
          <button
            key={g.name}
            className={`chip ${sel.kind === "group" && sel.name === g.name ? "active" : ""} ${meta.groups[g.name] ? "has-custom" : ""}`}
            onClick={() => setSel({ kind: "group", name: g.name })}
          >
            {g.label}
          </button>
        ))}
      </div>

      <Keyboard
        platform={snap.platform}
        pressed={pressed}
        selected={selectedKeys}
        highlighted={highlighted}
        customized={customized}
        onKeyClick={onKeyClick}
      />
      <p className="hint">
        Click a key to select and hear it · Shift-click to select several · Type anywhere to test · Dots mark keys with their own sound
      </p>

      <div className="editor">
        <div className="editor-head">
          <h2>{title}</h2>
          {sel.kind === "keys" && sel.codes.length === 1 && (
            <span className="muted">
              in group{" "}
              <button className="link" onClick={() => setSel({ kind: "group", name: groupOf(sel.codes[0]) })}>
                {groupLabel(groupOf(sel.codes[0]))}
              </button>
            </span>
          )}
        </div>
        <div className="editor-cols">
          {(["down", "up"] as Dir[]).map((dir) => (
            <DirEditor
              key={dir}
              dir={dir}
              sel={sel}
              meta={meta}
              choices={choices}
              busy={busy}
              onSet={(refs) => apply(dir, refs)}
              onImport={(cur) => importFile(dir, cur)}
              onRecord={() => setRecordFor(dir)}
            />
          ))}
        </div>
        {active.warnings.length > 0 && (
          <details className="warnings">
            <summary>
              {active.warnings.length} problem{active.warnings.length > 1 ? "s" : ""} loading this pack
            </summary>
            <ul>
              {active.warnings.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          </details>
        )}
        <SoundLibrary active={active} setActive={setActive} toasts={toasts} />
      </div>

      {recordFor && (
        <RecorderModal
          onClose={() => setRecordFor(null)}
          onSaved={async (file) => {
            const dir = recordFor;
            setRecordFor(null);
            const cur = currentRefs(meta, sel, dir);
            await apply(dir, [...cur, file]);
            toasts.info(`Saved “${refLabel(file)}” and assigned it`);
          }}
          toasts={toasts}
        />
      )}
    </div>
  );
}

function selTarget(sel: Selection): Target {
  if (sel.kind === "keys") return { kind: "key", name: sel.codes[0] };
  if (sel.kind === "group") return { kind: "group", name: sel.name };
  return { kind: "default" };
}

function currentRefs(meta: PackDetail["meta"], sel: Selection, dir: Dir): string[] {
  return slotFor(meta, selTarget(sel))[dir] ?? [];
}

function DirEditor({
  dir,
  sel,
  meta,
  choices,
  busy,
  onSet,
  onImport,
  onRecord,
}: {
  dir: Dir;
  sel: Selection;
  meta: PackDetail["meta"];
  choices: string[];
  busy: boolean;
  onSet: (refs: string[] | null) => void;
  onImport: (current: string[]) => void;
  onRecord: () => void;
}) {
  const own = slotFor(meta, selTarget(sel))[dir];
  const isOwn = own != null;

  let inherited: { refs: string[]; from: string } | null = null;
  if (!isOwn) {
    if (sel.kind === "keys") {
      const r = resolve(meta, sel.codes[0], dir);
      inherited = {
        refs: r.refs,
        from: r.from === "group" ? `${groupLabel(groupOf(sel.codes[0]))} group` : r.from === "default" ? "pack default" : "nothing",
      };
    } else if (sel.kind === "group") {
      inherited = { refs: meta.defaults?.[dir] ?? [], from: meta.defaults?.[dir] != null ? "pack default" : "nothing" };
    }
  }

  const refs = own ?? [];
  const shown = isOwn ? refs : inherited?.refs ?? [];

  return (
    <div className="dir-editor">
      <div className="dir-head">
        <h3>{dir === "down" ? "Key press" : "Key release"}</h3>
        {isOwn ? (
          sel.kind !== "default" && (
            <button className="link" disabled={busy} onClick={() => onSet(null)} title="Remove this setting and inherit instead">
              Reset to inherited
            </button>
          )
        ) : (
          <span className="muted small">
            {inherited && inherited.from !== "nothing" ? `Inherited from ${inherited.from}` : "Not set — silent"}
          </span>
        )}
      </div>

      <div className={`sound-list ${isOwn ? "" : "inherited"}`}>
        {shown.length === 0 && <div className="muted small">{isOwn ? "Silent" : "No sound"}</div>}
        {shown.map((r, i) => (
          <div className="sound-chip" key={`${r}-${i}`}>
            <button className="icon-btn" aria-label={`Play ${refLabel(r)}`} onClick={() => api.previewRef(r).catch(() => {})}>
              ▶
            </button>
            <span className="sound-name" title={r}>
              {refLabel(r)}
            </span>
            {isOwn && (
              <button
                className="icon-btn"
                aria-label={`Remove ${refLabel(r)}`}
                disabled={busy}
                onClick={() => onSet(refs.filter((_, j) => j !== i))}
              >
                ×
              </button>
            )}
          </div>
        ))}
      </div>
      {shown.length > 1 && <p className="muted small">A random variation plays on each press.</p>}

      <div className="dir-actions">
        <select
          value=""
          disabled={busy}
          onChange={(e) => {
            const v = e.target.value;
            if (v) onSet([...(isOwn ? refs : []), v]);
          }}
        >
          <option value="">{isOwn ? "Add a pack sound…" : "Use a pack sound…"}</option>
          {choices.map((c) => (
            <option key={c} value={c}>
              {refLabel(c)}
            </option>
          ))}
        </select>
        <button className="btn" disabled={busy} onClick={() => onImport(isOwn ? refs : [])}>
          Import file…
        </button>
        <button className="btn" disabled={busy} onClick={onRecord}>
          Record…
        </button>
        {!(isOwn && refs.length === 0) && (
          <button className="btn ghost" disabled={busy} onClick={() => onSet([])} title="Play nothing">
            Silence
          </button>
        )}
      </div>
    </div>
  );
}

function SoundLibrary({ active, setActive, toasts }: { active: PackDetail; setActive: (d: PackDetail | null) => void; toasts: Toasts }) {
  const used = useMemo(() => {
    const s = new Set<string>();
    const add = (v?: string[] | null) => v?.forEach((r) => s.add(r));
    const m = active.meta;
    add(m.defaults?.down);
    add(m.defaults?.up);
    Object.values(m.groups).forEach((g) => (add(g.down), add(g.up)));
    Object.values(m.keys).forEach((g) => (add(g.down), add(g.up)));
    return s;
  }, [active.meta]);

  return (
    <details className="library">
      <summary>Sounds in this pack ({active.files.length})</summary>
      <div className="library-grid">
        {active.files.map((f) => (
          <div className="sound-chip" key={f}>
            <button className="icon-btn" aria-label={`Play ${f}`} onClick={() => api.previewRef(f).catch((e) => toasts.error(errorText(e)))}>
              ▶
            </button>
            <span className="sound-name" title={f}>
              {refLabel(f)}
            </span>
            {!used.has(f) && <span className="tag">unused</span>}
            <button
              className="icon-btn"
              aria-label={`Delete ${f}`}
              onClick={async () => {
                if (!(await ask(`Delete “${refLabel(f)}” from this pack? Keys using it will lose it.`, { title: "Delete sound", kind: "warning" }))) return;
                try {
                  setActive(await api.deleteSound(f));
                } catch (e) {
                  toasts.error(errorText(e));
                }
              }}
            >
              🗑
            </button>
          </div>
        ))}
      </div>
    </details>
  );
}
