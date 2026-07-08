import { invoke } from "@tauri-apps/api/core";
import type { SnapshotSummary, CaptureResult, RestoreResult } from "../types/snapshot";

export async function takeSnapshot(name: string): Promise<CaptureResult> {
  return invoke<CaptureResult>("take_snapshot", { name });
}

export async function recaptureSnapshot(id: string): Promise<CaptureResult> {
  return invoke<CaptureResult>("recapture_snapshot", { id });
}

export async function listSnapshots(): Promise<SnapshotSummary[]> {
  return invoke<SnapshotSummary[]>("list_snapshots");
}

export async function restoreSnapshot(
  id: string,
  closeOthers = false
): Promise<RestoreResult> {
  return invoke<RestoreResult>("restore_snapshot", { id, closeOthers });
}

/** Whether the desktop the user is currently looking at is already captured somewhere. */
export async function isCurrentStateSaved(): Promise<boolean> {
  return invoke<boolean>("is_current_state_saved");
}

export async function deleteSnapshot(id: string): Promise<void> {
  return invoke<void>("delete_snapshot", { id });
}

export async function clearAllSnapshots(): Promise<void> {
  return invoke<void>("clear_all_snapshots");
}
