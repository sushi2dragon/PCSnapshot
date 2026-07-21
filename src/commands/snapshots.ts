import { invoke } from "@tauri-apps/api/core";
import type { Snapshot, SnapshotSummary, CaptureResult, RestoreResult, CloseResult, ActiveSession } from "../types/snapshot";

export async function takeSnapshot(name: string): Promise<CaptureResult> {
  return invoke<CaptureResult>("take_snapshot", { name });
}

export async function recaptureSnapshot(id: string): Promise<CaptureResult> {
  return invoke<CaptureResult>("recapture_snapshot", { id });
}

export async function listSnapshots(): Promise<SnapshotSummary[]> {
  return invoke<SnapshotSummary[]>("list_snapshots");
}
export async function getSnapshot(id: string): Promise<Snapshot> { return invoke<Snapshot>("get_snapshot", { id }); }
export async function closeAllWindows(): Promise<CloseResult> { return invoke<CloseResult>("close_all_windows"); }

export async function restoreSnapshot(
  id: string,
  closeOthers = false
): Promise<RestoreResult> {
  return invoke<RestoreResult>("restore_snapshot", { id, closeOthers });
}

/** Restore one captured application without closing or changing other apps. */
export async function restoreApp(id: string, exePath: string): Promise<RestoreResult> {
  return invoke<RestoreResult>("restore_app", { id, exePath });
}

/** Restore every captured File Explorer folder window without touching other apps. */
export async function restoreExplorerWindows(id: string): Promise<RestoreResult> {
  return invoke<RestoreResult>("restore_explorer_windows", { id });
}

/** Whether the desktop the user is currently looking at is already captured somewhere. */
export async function isCurrentStateSaved(): Promise<boolean> {
  return invoke<boolean>("is_current_state_saved");
}

export async function deleteSnapshot(id: string): Promise<void> {
  return invoke<void>("delete_snapshot", { id });
}

export async function renameSnapshot(id: string, name: string): Promise<SnapshotSummary> {
  return invoke<SnapshotSummary>("rename_snapshot", { id, name });
}

export async function clearAllSnapshots(): Promise<void> {
  return invoke<void>("clear_all_snapshots");
}

/** The snapshot the user is currently working in (last restored), or null. */
export async function getActiveSession(): Promise<ActiveSession | null> {
  return invoke<ActiveSession | null>("get_active_session");
}

/** The app's own icon as a PNG data URI, or null if it can't be read. */
export async function getAppIcon(exePath: string): Promise<string | null> {
  return invoke<string | null>("get_app_icon", { exePath });
}
