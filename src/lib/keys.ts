import type { Dir, PackFile, Target } from "./api";

export type Platform = "macos" | "windows" | "linux";

/** A key on the on-screen keyboard. `w` is width in key units. */
export interface KeyDef {
  code: string;
  label: string;
  w?: number;
}
export type Row = (KeyDef | { gap: number })[];

const k = (code: string, label: string, w = 1): KeyDef => ({ code, label, w });
const gap = (n: number) => ({ gap: n });
const letters = (s: string) => s.split("").map((c) => k(`Key${c}`, c));

export function mainRows(p: Platform): Row[] {
  const mac = p === "macos";
  return [
    // 1 + 12 F-keys + 3 gaps of 2/3 = 15 units, same as the other rows.
    [k("Escape", "esc"), gap(2 / 3), ...[1, 2, 3, 4].map((n) => k(`F${n}`, `F${n}`)), gap(2 / 3),
      ...[5, 6, 7, 8].map((n) => k(`F${n}`, `F${n}`)), gap(2 / 3), ...[9, 10, 11, 12].map((n) => k(`F${n}`, `F${n}`))],
    [k("Backquote", "`"), ...["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"].map((d) => k(`Digit${d}`, d)),
      k("Minus", "-"), k("Equal", "="), k("Backspace", mac ? "delete" : "backspace", 2)],
    [k("Tab", "tab", 1.5), ...letters("QWERTYUIOP"), k("BracketLeft", "["), k("BracketRight", "]"), k("Backslash", "\\", 1.5)],
    [k("CapsLock", "caps", 1.75), ...letters("ASDFGHJKL"), k("Semicolon", ";"), k("Quote", "'"), k("Enter", mac ? "return" : "enter", 2.25)],
    [k("ShiftLeft", "shift", 2.25), ...letters("ZXCVBNM"), k("Comma", ","), k("Period", "."), k("Slash", "/"), k("ShiftRight", "shift", 2.75)],
    mac
      ? [k("Fn", "fn", 1.25), k("ControlLeft", "⌃", 1.25), k("AltLeft", "⌥", 1.25), k("MetaLeft", "⌘", 1.25),
          k("Space", "", 6.25), k("MetaRight", "⌘", 1.25), k("AltRight", "⌥", 1.25), k("ControlRight", "⌃", 1.25)]
      : [k("ControlLeft", "ctrl", 1.25), k("MetaLeft", p === "windows" ? "win" : "super", 1.25), k("AltLeft", "alt", 1.25),
          k("Space", "", 6.25), k("AltRight", "alt", 1.25), k("MetaRight", p === "windows" ? "win" : "super", 1.25),
          k("ContextMenu", "menu", 1.25), k("ControlRight", "ctrl", 1.25)],
  ];
}

export const navRows: Row[] = [
  [k("PrintScreen", "prt"), k("ScrollLock", "scr"), k("Pause", "pse")],
  [k("Insert", "ins"), k("Home", "home"), k("PageUp", "pg up")],
  [k("Delete", "del"), k("End", "end"), k("PageDown", "pg dn")],
  [gap(3)],
  [gap(1), k("ArrowUp", "↑")],
  [k("ArrowLeft", "←"), k("ArrowDown", "↓"), k("ArrowRight", "→")],
];

export function layoutCodes(p: Platform): string[] {
  return [...mainRows(p), ...navRows].flatMap((r) => r.flatMap((i) => ("code" in i ? [i.code] : [])));
}

export const GROUPS: { name: string; label: string }[] = [
  { name: "letters", label: "Letters" },
  { name: "digits", label: "Number row" },
  { name: "punctuation", label: "Punctuation" },
  { name: "modifiers", label: "Modifiers" },
  { name: "space", label: "Space bar" },
  { name: "enter", label: "Enter / Return" },
  { name: "backspace", label: "Backspace & Delete" },
  { name: "tab", label: "Tab" },
  { name: "escape", label: "Escape" },
  { name: "arrows", label: "Arrow keys" },
  { name: "navigation", label: "Home / End / Page" },
  { name: "function", label: "F-keys" },
  { name: "numpad", label: "Numpad" },
  { name: "other", label: "Other keys" },
];

/** Mirror of `keyhook::group_of`. */
export function groupOf(code: string): string {
  if (code.startsWith("Key")) return "letters";
  if (code.startsWith("Digit")) return "digits";
  if (code === "NumpadEnter") return "enter";
  if (code.startsWith("Numpad") || code === "NumLock") return "numpad";
  if (["Minus", "Equal", "BracketLeft", "BracketRight", "Backslash", "Semicolon", "Quote", "Backquote", "Comma", "Period", "Slash", "IntlBackslash"].includes(code))
    return "punctuation";
  if (["ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight", "CapsLock", "Fn"].includes(code))
    return "modifiers";
  if (code === "Space") return "space";
  if (code === "Enter") return "enter";
  if (code === "Backspace" || code === "Delete") return "backspace";
  if (code === "Tab") return "tab";
  if (code === "Escape") return "escape";
  if (code.startsWith("Arrow")) return "arrows";
  if (["Home", "End", "PageUp", "PageDown", "Insert"].includes(code)) return "navigation";
  if (/^F\d{1,2}$/.test(code)) return "function";
  return "other";
}

export function groupLabel(name: string) {
  return GROUPS.find((g) => g.name === name)?.label ?? name;
}

export interface Resolved {
  refs: string[];
  from: "key" | "group" | "default" | "none";
}

/** Mirror of the Rust resolution order: key -> group -> defaults. */
export function resolve(meta: PackFile, code: string, dir: Dir): Resolved {
  const kv = meta.keys[code]?.[dir];
  if (kv != null) return { refs: kv, from: "key" };
  const gv = meta.groups[groupOf(code)]?.[dir];
  if (gv != null) return { refs: gv, from: "group" };
  const dv = meta.defaults?.[dir];
  if (dv != null) return { refs: dv, from: "default" };
  return { refs: [], from: "none" };
}

export function slotFor(meta: PackFile, t: Target) {
  if (t.kind === "default") return meta.defaults ?? {};
  if (t.kind === "group") return meta.groups[t.name] ?? {};
  return meta.keys[t.name] ?? {};
}

/** Pretty name for a sound reference. */
export function refLabel(ref: string) {
  if (ref.startsWith("#")) return `segment ${ref.slice(1)}`;
  return ref.replace(/\.(wav|mp3|ogg|oga|flac|m4a|aac)$/i, "");
}

export function codeLabel(code: string) {
  return code
    .replace(/^Key/, "")
    .replace(/^Digit/, "")
    .replace(/^Numpad/, "Numpad ")
    .replace(/(Left|Right)$/, " ($1)")
    .replace(/^Arrow/, "Arrow ");
}

/** Convert a browser KeyboardEvent to a Tauri accelerator string. */
export function acceleratorFromEvent(e: KeyboardEvent, p: Platform): string | null {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push(p === "macos" ? "Ctrl" : "CmdOrCtrl");
  if (e.metaKey) mods.push(p === "macos" ? "CmdOrCtrl" : "Super");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  const c = e.code;
  let key: string | null = null;
  if (c.startsWith("Key")) key = c.slice(3);
  else if (c.startsWith("Digit")) key = c.slice(5);
  else if (/^F\d{1,2}$/.test(c)) key = c;
  else if (["Space", "Enter", "Tab", "Backspace", "Escape", "Home", "End", "PageUp", "PageDown", "Insert", "Delete"].includes(c)) key = c;
  else if (c.startsWith("Arrow")) key = c.replace("Arrow", "");
  if (!key) return null;
  if (mods.length === 0 && !/^F\d/.test(key)) return null; // bare letters would hijack typing
  return [...mods, key].join("+");
}

export function prettyAccelerator(acc: string | null, p: Platform) {
  if (!acc) return "None";
  return acc
    .split("+")
    .map((part) => {
      if (part === "CmdOrCtrl") return p === "macos" ? "⌘" : "Ctrl";
      if (p === "macos" && part === "Shift") return "⇧";
      if (p === "macos" && part === "Alt") return "⌥";
      if (p === "macos" && part === "Ctrl") return "⌃";
      return part;
    })
    .join(p === "macos" ? "" : " + ");
}
