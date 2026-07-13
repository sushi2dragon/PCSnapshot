import { useCallback, useEffect, useState } from "react";
import { getActiveSession } from "../commands/snapshots";
export function useActiveSession() {
  const [activeId, setActiveId] = useState<string | null>(null);
  const refresh = useCallback(
    () => getActiveSession().then((m) => setActiveId(m?.id ?? null)).catch(() => setActiveId(null)),
    []
  );
  useEffect(() => { refresh(); }, [refresh]);
  return { activeId, refresh };
}
