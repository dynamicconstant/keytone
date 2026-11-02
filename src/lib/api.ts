import { invoke } from "@tauri-apps/api/core";

export type Dir = "down" | "up";

export interface SlotDef {
  down?: string[] | null;
  up?: string[] | null;
}

export interface RandomCfg {
  pitch_cents: number;
  gain_db: number;
}

export interface PackFile {
  schema: number;
  id: string;
  name: string;
  author: string;
  description: string;
  cover?: string | null;
  volume: number;
  trim_silence: boolean;
  random: RandomCfg;
  sprite?: { file: string; segments: Record<string, [number, number]> } | null;
  defaults: SlotDef;
  groups: Record<string, SlotDef>;
  keys: Record<string, SlotDef>;
}

export interface PackDetail {
  id: string;
  meta: PackFile;
  files: string[];
  warnings: string[];
}

export interface PackSummary {
  id: string;
  name: string;
  author: string;
  description: string;
  has_cover: boolean;
  key_overrides: number;
}

export interface Settings {
  active_pack: string | null;
  volume: number;
  enabled: boolean;
  toggle_hotkey: string | null;
  output_device: string | null;
  buffer_frames: number | null;
  launch_at_login: boolean;
}

export interface EngineInfo {
  device: string;
  sample_rate: number;
  channels: number;
  buffer_frames: number | null;
  latency_ms: number | null;
  error: string | null;
  note: string | null;
}

export interface HookStatus {
  running: boolean;
  permission: "granted" | "denied" | "not_required" | "";
  error: string | null;
  error_kind: "permission" | "unsupported" | "other" | null;
}

export interface Snapshot {
  settings: Settings;
  packs: PackSummary[];
  active: PackDetail | null;
  active_error: string | null;
  engine: EngineInfo;
  hook: HookStatus;
  platform: "macos" | "windows" | "linux";
  version: string;
}

export type Target = { kind: "default" } | { kind: "group"; name: string } | { kind: "key"; name: string };

export interface RecStatus {
  active: boolean;
  level: number;
  elapsed: number;
  max_secs: number;
}

export interface SettingsPatch {
  volume?: number;
  enabled?: boolean;
  toggle_hotkey?: string | null;
  output_device?: string | null;
  buffer_frames?: number | null;
  launch_at_login?: boolean;
}

export const api = {
  getState: () => invoke<Snapshot>("get_state"),
  setEnabled: (on: boolean) => invoke<void>("set_enabled", { on }),
  setVolume: (volume: number) => invoke<void>("set_volume", { volume }),
  setKeyFeedback: (on: boolean) => invoke<void>("set_key_feedback", { on }),
  previewKey: (code: string) => invoke<void>("preview_key", { code }),
  previewRef: (reference: string) => invoke<void>("preview_ref", { reference }),

  setActivePack: (id: string) => invoke<PackDetail | null>("set_active_pack", { id }),
  setMapping: (targets: Target[], dir: Dir, refs: string[] | null) =>
    invoke<PackDetail | null>("set_mapping", { targets, dir, refs }),
  updatePackMeta: (patch: Partial<Pick<PackFile, "name" | "author" | "description" | "volume" | "trim_silence" | "random">>) =>
    invoke<PackDetail | null>("update_pack_meta", { patch }),
  importSound: (path: string) => invoke<string>("import_sound", { path }),
  deleteSound: (file: string) => invoke<PackDetail | null>("delete_sound", { file }),
  createPack: (name: string) => invoke<string>("create_pack", { name }),
  duplicatePack: (id: string, name: string) => invoke<string>("duplicate_pack", { id, name }),
  deletePack: (id: string) => invoke<void>("delete_pack", { id }),
  importPack: (path: string) => invoke<string>("import_pack", { path }),
  exportPack: (id: string, dest: string) => invoke<void>("export_pack", { id, dest }),

  startRecording: () => invoke<void>("start_recording"),
  recordingStatus: () => invoke<RecStatus>("recording_status"),
  stopRecording: () => invoke<number>("stop_recording"),
  previewTake: () => invoke<void>("preview_take"),
  saveTake: (name: string) => invoke<string>("save_take", { name }),
  discardTake: () => invoke<void>("discard_take"),

  listDevices: () => invoke<string[]>("list_devices"),
  updateSettings: (patch: SettingsPatch) => invoke<Snapshot>("update_settings", { patch }),
  requestPermission: () => invoke<HookStatus>("request_permission"),
  retryHook: () => invoke<HookStatus>("retry_hook"),
  openPermissionSettings: () => invoke<void>("open_permission_settings"),
  restartApp: () => invoke<void>("restart_app"),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
