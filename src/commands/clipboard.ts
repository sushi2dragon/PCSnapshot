import { invoke } from "@tauri-apps/api/core";
import type { ClipboardCacheRow } from "../types/snapshot";

/** All captured clipboard items across snapshots + pre-restore backups. Empty when opted out. */
export async function listClipboardCache(): Promise<ClipboardCacheRow[]> {
  return invoke<ClipboardCacheRow[]>("list_clipboard_cache");
}

/** Re-copy a stored clipboard item to the live clipboard. `source` is "snapshot" or "backup". */
export async function copyClipboardItem(
  source: string,
  containerId: string,
  itemId: string
): Promise<void> {
  return invoke<void>("copy_clipboard_item", { source, containerId, itemId });
}

/** Reseed the live Win+V history from a snapshot's whole clipboard block.
 *  Resolves with any warnings (empty array = clean restore). */
export async function restoreClipboard(snapshotId: string): Promise<string[]> {
  return invoke<string[]>("restore_clipboard", { id: snapshotId });
}

/** Delete one stored clipboard item from its snapshot block or backup entry. */
export async function deleteClipboardEntry(
  source: string,
  containerId: string,
  itemId: string
): Promise<void> {
  return invoke<void>("delete_clipboard_entry", { source, containerId, itemId });
}
