import { useState, useCallback, useEffect } from "react";
import * as config from "../commands/config";

export function useIgnoreList() {
  const [list, setList] = useState<string[]>([]);
  const [running, setRunning] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [ignoreList, runningProcs] = await Promise.all([
        config.getIgnoreList(),
        config.getRunningProcesses(),
      ]);
      setList(ignoreList);
      setRunning(runningProcs);
    } catch {
      // best-effort
    }
    setLoading(false);
  }, []);

  // Initial load. setState only happens in promise callbacks (loading starts true),
  // which keeps the effect body free of synchronous setState calls.
  useEffect(() => {
    let cancelled = false;
    Promise.all([config.getIgnoreList(), config.getRunningProcesses()])
      .then(([ignoreList, runningProcs]) => {
        if (cancelled) return;
        setList(ignoreList);
        setRunning(runningProcs);
      })
      .catch(() => {
        // best-effort
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const add = useCallback(
    async (exeName: string) => {
      await config.addToIgnoreList(exeName);
      await refresh();
    },
    [refresh]
  );

  const remove = useCallback(
    async (exeName: string) => {
      await config.removeFromIgnoreList(exeName);
      await refresh();
    },
    [refresh]
  );

  return { list, running, loading, add, remove, refresh };
}
