export interface ProcessInfo {
  name: string;
  pid: number;
  exe_path: string;
  cmd_line: string;
  classification: string;
}

export interface WindowInfo {
  title: string;
  position: { x: number; y: number };
  size: { width: number; height: number };
  state: "normal" | "minimized" | "maximized";
  monitor_index: number;
  /** Full exe path of the owning process. Added in schema v2; may be empty for old snapshots. */
  exe_path?: string;
}

export interface ExplorerWindow {
  path: string;
  path_kind: "filesystem" | "virtual";
  title: string;
  position: { x: number; y: number };
  size: { width: number; height: number };
  state: "normal" | "minimized" | "maximized";
  monitor_index: number;
}

export interface TerminalSession {
  shell: string;
  cwd: string;
  history: string[];
  window_title: string;
}

export interface BrowserTab {
  url: string;
  title: string;
  index: number;
  active: boolean;
  pinned: boolean;
  muted: boolean;
  discarded: boolean;
  group_key: string | null;
  restorable: boolean;
}

export interface BrowserTabGroup {
  key: string;
  title: string;
  color: string;
  collapsed: boolean;
  index: number | null;
}

export interface BrowserWindow {
  ordinal: number;
  bounds: { left: number | null; top: number | null; width: number | null; height: number | null };
  state: string;
  focused: boolean;
  tabs: BrowserTab[];
  groups: BrowserTabGroup[];
}

export interface BrowserSession {
  protocol_version: number;
  browser: { family: string; profile_instance_id: string };
  captured_at: string;
  capabilities: { tab_groups: boolean };
  windows: BrowserWindow[];
}

export interface ContextClue {
  type: string;
  value: string;
  confidence: number;
  source: string;
}

export type ClipboardKind = "text" | "image";

export interface ClipboardItem {
  id: string;
  kind: ClipboardKind;
  /** 0 = oldest; highest order is the newest / top of the Win+V stack. */
  order: number;
  text?: string;
  /** PNG sidecar filename (image items). */
  sidecar?: string;
  source: string;
  byte_size: number;
}

export interface ClipboardBlock {
  captured_at: string;
  items: ClipboardItem[];
}

/** One flattened row for the settings Clipboard Cache panel. */
export interface ClipboardCacheRow {
  row_id: string;
  source: "snapshot" | "backup";
  container_id: string;
  label: string;
  created_at: string;
  kind: ClipboardKind;
  order: number;
  text?: string | null;
  /** Absolute path to the image sidecar; run through convertFileSrc. */
  sidecar_path?: string | null;
  item_id: string;
}

export interface Snapshot {
  schema_version?: number;
  id: string;
  name: string;
  timestamp: string;
  processes: ProcessInfo[];
  windows: WindowInfo[];
  explorer_windows?: ExplorerWindow[];
  context_clues: ContextClue[];
  restore_hints: string[];
  warnings: string[];
  thumbnail_path: string;
  terminal_sessions?: TerminalSession[];
  browser_sessions?: BrowserSession[];
  /** Present only when the clipboard opt-in was on at capture time. */
  clipboard?: ClipboardBlock;
}

export interface SnapshotSummary {
  id: string;
  name: string;
  timestamp: string;
  thumbnail_path: string;
  warning_count: number;
  /** Captured apps (processes, plus one for File Explorer if any folders were captured). */
  app_count: number;
  /** Distinct monitors spanned by the captured windows (at least 1). */
  monitor_count: number;
  /** Exe paths of the first few distinct captured apps, for the tile icon stack. */
  top_apps: string[];
}

export interface CaptureResult {
  snapshot: SnapshotSummary;
  warnings: string[];
}

export interface RestoreResult {
  success: boolean;
  message: string;
  /** Hard failures: apps that could not be launched. */
  failed_items: string[];
  /** Soft warnings: windows not repositioned, plus extras that refused to close. */
  warnings: string[];
  /** Windows closed because they were not part of the snapshot (clean restore only). */
  closed_items: string[];
}

export interface ActivityEvent {
  id: string; timestamp: string; kind: string; snapshot_name: string | null;
  status: "success" | "warning" | "failed"; summary: string; detail_lines: string[];
}
export interface CloseResult { closed: string[]; refused: string[] }

export interface ActiveSession {
  id: string;
  timestamp: string;
}

/** Browser Companion health: host registration plus who is connected right now. */
export interface CompanionReport {
  /** The native-messaging relay exists beside the app; nothing works without it. */
  host_installed: boolean;
  host_path: string;
  registered_browsers: string[];
  errors: string[];
  connected_browsers: string[];
}
