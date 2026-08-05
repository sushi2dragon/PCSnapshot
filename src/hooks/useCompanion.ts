import { useCallback, useEffect, useState } from "react";
import { companionStatus, browserCompanionOverview } from "../commands/snapshots";
import type { CompanionBrowser, CompanionReport } from "../types/snapshot";

/** Shared Browser Companion state for Settings: live host/connection health plus
 *  the per-browser last-captured-tabs overview. Drives both the Opt-Ins hub row
 *  (install button vs. "connected" badge) and the Browser Companion detail page. */
export function useCompanion() {
  const [report, setReport] = useState<CompanionReport | null>(null);
  const [browsers, setBrowsers] = useState<CompanionBrowser[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [r, b] = await Promise.all([companionStatus(), browserCompanionOverview()]);
      setReport(r);
      setBrowsers(b);
    } catch {
      // best-effort; leave prior state in place
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // A live native-messaging connection to at least one browser.
  const connected = (report?.connected_browsers.length ?? 0) > 0;
  // Something worth showing on the detail page: connected now, or ever captured.
  const active = connected || browsers.length > 0;

  return { report, browsers, loading, connected, active, refresh };
}
