import { useCallback, useEffect, useState } from "react";
import { getCaptureClipboard, setCaptureClipboard } from "../commands/config";
import {
  listClipboardCache,
  copyClipboardItem,
  deleteClipboardEntry,
} from "../commands/clipboard";
import type { ClipboardCacheRow } from "../types/snapshot";

/** Settings-panel state for the Clipboard Cache: the master opt-in plus the
 *  aggregated list of captured items (empty when opted out). */
export function useClipboard() {
  const [enabled, setEnabled] = useState(false);
  const [rows, setRows] = useState<ClipboardCacheRow[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const on = await getCaptureClipboard();
      setEnabled(on);
      setRows(on ? await listClipboardCache() : []);
    } catch {
      // best-effort
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggle = useCallback(async () => {
    const next = !enabled;
    await setCaptureClipboard(next);
    await refresh();
  }, [enabled, refresh]);

  const copy = useCallback(async (row: ClipboardCacheRow) => {
    await copyClipboardItem(row.source, row.container_id, row.item_id);
  }, []);

  const remove = useCallback(
    async (row: ClipboardCacheRow) => {
      await deleteClipboardEntry(row.source, row.container_id, row.item_id);
      await refresh();
    },
    [refresh]
  );

  return { enabled, rows, loading, toggle, copy, remove, refresh };
}
