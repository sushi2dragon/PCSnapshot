import { useEffect } from "react";
import type { RestoreResult } from "../types/snapshot";

interface RestoreReportModalProps {
  result: RestoreResult | null;
  onDismiss: () => void;
}

export function RestoreReportModal({ result, onDismiss }: RestoreReportModalProps) {
  // Keyboard: Esc or Enter dismisses the report.
  useEffect(() => {
    if (!result) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape" || e.key === "Enter") {
        // preventDefault stops a focused button's own Enter activation from
        // firing onDismiss a second time.
        e.preventDefault();
        onDismiss();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [result, onDismiss]);

  if (!result) return null;

  const hasFailures = result.failed_items.length > 0;
  const hasWarnings = result.warnings.length > 0;
  const hasClosed = result.closed_items.length > 0;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ backgroundColor: "rgba(0,0,0,0.6)" }}
      onClick={onDismiss}
    >
      <div
        className="w-full max-w-md rounded-xl shadow-2xl overflow-hidden"
        style={{ backgroundColor: "var(--bg-card, #252528)", border: "1px solid rgba(255,255,255,0.08)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="px-6 py-5 border-b" style={{ borderColor: "rgba(255,255,255,0.08)" }}>
          <div className="flex items-start gap-3">
            <div
              className="mt-0.5 flex-shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-xs font-bold"
              style={{
                backgroundColor: hasFailures
                  ? "rgba(239,68,68,0.2)"
                  : hasWarnings
                  ? "rgba(251,146,60,0.2)"
                  : "rgba(75,191,195,0.2)",
                color: hasFailures ? "#f87171" : hasWarnings ? "#fb923c" : "#4bbfc3",
              }}
            >
              {hasFailures ? "✕" : hasWarnings ? "!" : "✓"}
            </div>
            <div>
              <h2 className="text-sm font-semibold text-white leading-snug">
                {!result.success
                  ? "Restore partially failed"
                  : hasWarnings
                  ? "Restore completed with warnings"
                  : "Restore complete"}
              </h2>
              <p className="mt-1 text-xs" style={{ color: "rgba(255,255,255,0.45)" }}>
                {result.message}
              </p>
            </div>
          </div>
        </div>

        {/* Body */}
        <div className="px-6 py-4 space-y-4 max-h-72 overflow-y-auto">
          {hasFailures && (
            <section>
              <h3 className="text-xs font-semibold uppercase tracking-wider mb-2" style={{ color: "#f87171" }}>
                Could not launch
              </h3>
              <ul className="space-y-1.5">
                {result.failed_items.map((item, i) => (
                  <FailureItem key={i} text={item} color="#f87171" />
                ))}
              </ul>
            </section>
          )}

          {hasWarnings && (
            <section>
              <h3 className="text-xs font-semibold uppercase tracking-wider mb-2" style={{ color: "#fb923c" }}>
                Could not reposition
              </h3>
              <ul className="space-y-1.5">
                {result.warnings.map((item, i) => (
                  <FailureItem key={i} text={item} color="#fb923c" />
                ))}
              </ul>
            </section>
          )}

          {hasClosed && (
            <section>
              <h3 className="text-xs font-semibold uppercase tracking-wider mb-2" style={{ color: "#4bbfc3" }}>
                Closed (not in snapshot)
              </h3>
              <ul className="space-y-1.5">
                {result.closed_items.map((item, i) => (
                  <FailureItem key={i} text={item} color="#4bbfc3" />
                ))}
              </ul>
            </section>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t flex justify-end" style={{ borderColor: "rgba(255,255,255,0.08)" }}>
          <button
            onClick={onDismiss}
            className="px-4 py-1.5 rounded-md text-sm font-medium transition-colors"
            style={{
              backgroundColor: "rgba(255,255,255,0.08)",
              color: "rgba(255,255,255,0.7)",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "rgba(255,255,255,0.13)")}
            onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "rgba(255,255,255,0.08)")}
          >
            Dismiss
          </button>
        </div>
      </div>
    </div>
  );
}

function FailureItem({ text, color }: { text: string; color: string }) {
  // Split "AppName: reason" into name + reason for nicer rendering.
  const colonIdx = text.indexOf(": ");
  const name = colonIdx > -1 ? text.slice(0, colonIdx) : text;
  const reason = colonIdx > -1 ? text.slice(colonIdx + 2) : null;

  return (
    <li
      className="flex flex-col gap-0.5 px-3 py-2 rounded-lg text-xs"
      style={{ backgroundColor: "rgba(255,255,255,0.04)" }}
    >
      <span className="font-medium" style={{ color }}>{name}</span>
      {reason && (
        <span style={{ color: "rgba(255,255,255,0.45)" }}>{reason}</span>
      )}
    </li>
  );
}
