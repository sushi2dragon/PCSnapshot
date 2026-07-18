import { useCallback, useEffect, useState } from "react";
import { listActivity } from "../commands/activity";
import type { ActivityEvent } from "../types/snapshot";
export function useActivity() {
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const refresh = useCallback(() => listActivity().then(setEvents).catch(() => setEvents([])), []);
  useEffect(() => { refresh(); }, [refresh]);
  return { events, refresh };
}
