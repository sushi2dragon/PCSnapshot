import { useEffect, useRef, useState } from "react";
import { readActivityLog } from "../commands/activity";
import type { ActivityEvent } from "../types/snapshot";

interface ActivityLogModalProps {
  /// The event whose "Show logs" was clicked; null closes the viewer.
  event: ActivityEvent | null;
  onDismiss: () => void;
}

/// Raw activity-log viewer. The panel already prints an event's detail lines,
/// so the value here is the unparsed record plus its surrounding history —
/// enough to paste into a bug report without hunting for the file on disk.
export function ActivityLogModal({ event, onDismiss }: ActivityLogModalProps) {
  const [log, setLog] = useState<{ path: string; text: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<number | null>(null);
  const anchor = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!event) return;
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); onDismiss(); } };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [event, onDismiss]);

  useEffect(() => {
    if (!event) { setLog(null); setError(null); return; }
    let alive = true;
    readActivityLog(200)
      .then(l => { if (alive) setLog(l); })
      .catch(e => { if (alive) setError(String(e)); });
    return () => { alive = false; };
  }, [event]);

  // Bring the clicked event's line into view once the log has rendered.
  useEffect(() => { anchor.current?.scrollIntoView({ block: "center" }); }, [log]);

  useEffect(() => () => { if (copyTimer.current) window.clearTimeout(copyTimer.current); }, []);

  if (!event) return null;

  const lines = log ? log.text.split("\n").filter(Boolean) : [];

  const copy = () => {
    navigator.clipboard.writeText(log?.text ?? "").then(() => {
      setCopied(true);
      if (copyTimer.current) window.clearTimeout(copyTimer.current);
      copyTimer.current = window.setTimeout(() => setCopied(false), 1400);
    }).catch(() => {});
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center" style={{ backgroundColor: "rgba(0,0,0,0.6)" }} onClick={onDismiss}>
      <div
        className="w-full max-w-3xl rounded-xl shadow-2xl overflow-hidden flex flex-col"
        style={{ backgroundColor: "var(--bg-card, #252528)", border: "1px solid rgba(255,255,255,0.08)", maxHeight: "78vh" }}
        onClick={e => e.stopPropagation()}
      >
        <div className="px-6 py-4 border-b" style={{ borderColor: "rgba(255,255,255,0.08)" }}>
          <h2 className="text-sm font-semibold text-white">Activity log</h2>
          <p className="mt-1 text-xs break-all" style={{ color: "rgba(255,255,255,0.35)" }}>{log?.path ?? "…"}</p>
        </div>

        <div className="px-6 py-4 overflow-auto flex-1" style={{ font: "11px var(--font-mono)", lineHeight: 1.55 }}>
          {error && <p style={{ color: "#f87171" }}>{error}</p>}
          {!error && !log && <p style={{ color: "rgba(255,255,255,0.45)" }}>Reading log…</p>}
          {!error && log && lines.length === 0 && <p style={{ color: "rgba(255,255,255,0.45)" }}>The log is empty.</p>}
          {lines.map((line, i) => {
            const current = line.includes(`"${event.id}"`);
            return (
              <div
                key={i}
                ref={current ? anchor : undefined}
                className="whitespace-pre-wrap break-all px-2 py-1 rounded"
                style={{
                  color: current ? "#e6eefc" : "rgba(167,185,207,0.55)",
                  backgroundColor: current ? "rgba(75,191,195,0.10)" : "transparent",
                  borderLeft: current ? "2px solid #4bbfc3" : "2px solid transparent",
                }}
              >
                {line}
              </div>
            );
          })}
        </div>

        <div className="px-6 py-4 border-t flex justify-end gap-2" style={{ borderColor: "rgba(255,255,255,0.08)" }}>
          <button
            onClick={copy}
            disabled={!log?.text}
            className="px-4 py-1.5 rounded-md text-sm font-medium"
            style={{ backgroundColor: "rgba(255,255,255,0.08)", color: copied ? "#4bbfc3" : "rgba(255,255,255,0.7)" }}
          >
            {copied ? "Copied" : "Copy log"}
          </button>
          <button
            onClick={onDismiss}
            className="px-4 py-1.5 rounded-md text-sm font-medium"
            style={{ backgroundColor: "rgba(255,255,255,0.08)", color: "rgba(255,255,255,0.7)" }}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
