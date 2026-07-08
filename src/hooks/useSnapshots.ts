import { useState, useCallback, useEffect } from "react";
import type { SnapshotSummary, RestoreResult } from "../types/snapshot";
import * as commands from "../commands/snapshots";

export function useSnapshots() {
  const [snapshots, setSnapshots] = useState<SnapshotSummary[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const list = await commands.listSnapshots();
      setSnapshots(list);
    } catch {
      setSnapshots([]);
    }
    setLoading(false);
  }, []);

  // Initial load. setState only happens in promise callbacks (loading starts true),
  // which keeps the effect body free of synchronous setState calls.
  useEffect(() => {
    let cancelled = false;
    commands
      .listSnapshots()
      .then((list) => {
        if (!cancelled) setSnapshots(list);
      })
      .catch(() => {
        if (!cancelled) setSnapshots([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const capture = useCallback(
    async (name: string): Promise<string[]> => {
      const result = await commands.takeSnapshot(name);
      setSnapshots((prev) => [result.snapshot, ...prev]);
      return result.warnings;
    },
    []
  );

  const restore = useCallback(
    async (id: string, closeOthers = false): Promise<RestoreResult> => {
      return commands.restoreSnapshot(id, closeOthers);
    },
    []
  );

  const recapture = useCallback(
    async (id: string): Promise<string[]> => {
      const result = await commands.recaptureSnapshot(id);
      setSnapshots((prev) =>
        prev.map((s) => (s.id === id ? result.snapshot : s))
      );
      return result.warnings;
    },
    []
  );

  const remove = useCallback(
    async (id: string): Promise<void> => {
      await commands.deleteSnapshot(id);
      setSnapshots((prev) => prev.filter((s) => s.id !== id));
    },
    []
  );

  return { snapshots, loading, capture, recapture, restore, remove, refresh };
}
