import { useEffect, useRef, useState } from "react";
import { mainRows, navRows, type KeyDef, type Platform, type Row } from "../lib/keys";

interface Props {
  platform: Platform;
  pressed: Set<string>;
  selected: Set<string>;
  highlighted: Set<string>;
  customized: Set<string>;
  onKeyClick: (code: string, e: React.MouseEvent) => void;
}

const MAIN_UNITS = 15;
const NAV_UNITS = 3;
const GUTTER_UNITS = 0.6;

export default function Keyboard(props: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [unit, setUnit] = useState(40);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const w = entry.contentRect.width;
      setUnit(Math.max(26, Math.min(48, w / (MAIN_UNITS + NAV_UNITS + GUTTER_UNITS + 0.3))));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  return (
    <div className="keyboard-wrap" ref={ref}>
      <div className="keyboard" style={{ ["--u" as string]: `${unit}px` }}>
        <Block rows={mainRows(props.platform)} {...props} />
        <div style={{ width: unit * GUTTER_UNITS }} />
        <Block rows={navRows} {...props} />
      </div>
    </div>
  );
}

function Block({ rows, ...p }: { rows: Row[] } & Props) {
  return (
    <div className="kb-block">
      {rows.map((row, i) => (
        <div className={`kb-row ${i === 0 ? "kb-frow" : ""}`} key={i}>
          {row.map((item, j) =>
            "gap" in item ? (
              <div key={j} style={{ width: `calc(var(--u) * ${item.gap})` }} />
            ) : (
              <Key key={item.code} def={item} {...p} />
            ),
          )}
        </div>
      ))}
    </div>
  );
}

function Key({ def, pressed, selected, highlighted, customized, onKeyClick }: { def: KeyDef } & Props) {
  const cls = [
    "key",
    pressed.has(def.code) && "pressed",
    selected.has(def.code) && "selected",
    highlighted.has(def.code) && "highlighted",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <button
      className={cls}
      style={{ width: `calc(var(--u) * ${def.w ?? 1} - 4px)` }}
      onClick={(e) => onKeyClick(def.code, e)}
      title={def.code}
    >
      <span className="key-label">{def.label}</span>
      {customized.has(def.code) && <span className="key-dot" aria-label="custom sound" />}
    </button>
  );
}
