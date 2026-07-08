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

export interface TerminalSession {
  shell: string;
  cwd: string;
  history: string[];
  window_title: string;
}

export interface ContextClue {
  type: string;
  value: string;
  confidence: number;
  source: string;
}

export interface Snapshot {
  id: string;
  name: string;
  timestamp: string;
  processes: ProcessInfo[];
  windows: WindowInfo[];
  context_clues: ContextClue[];
  restore_hints: string[];
  warnings: string[];
  thumbnail_path: string;
  terminal_sessions?: TerminalSession[];
}

export interface SnapshotSummary {
  id: string;
  name: string;
  timestamp: string;
  thumbnail_path: string;
  warning_count: number;
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
