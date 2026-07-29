import { useCallback, useEffect, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import {
  getCurrentWindow,
  cursorPosition,
  LogicalSize,
  monitorFromPoint,
  PhysicalPosition,
  primaryMonitor,
} from "@tauri-apps/api/window";
import type { RestoreResult } from "../types/snapshot";

const OVERLAY_WIDTH = 440;
const AUTO_HIDE_MS = 8_000;

interface RestoreReportEnvelope {
  id: string;
  report: RestoreResult;
}

const overlayWindow = getCurrentWindow();

export function ErrorOverlay() {
  const [report, setReport] = useState<RestoreResult | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const generationRef = useRef(0);
  const dismiss = useCallback(() => {
    generationRef.current += 1;
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    overlayWindow
      .hide()
      .catch(() => {})
      .finally(() => setReport(null));
  }, [overlayWindow]);

  useEffect(() => {
    const surfaces = [document.documentElement, document.body, document.getElementById("root")];
    const previous = surfaces.map((element) => element?.style.background ?? "");
    surfaces.forEach((element) => {
      if (element) element.style.background = "transparent";
    });

    return () => {
      surfaces.forEach((element, index) => {
        if (element) element.style.background = previous[index];
      });
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    listen<RestoreReportEnvelope>("restore-report", ({ payload }) => {
      if (disposed) return;
      setReport(payload.report);
      emitTo("main", "restore-report-received", payload.id).catch(() => {});
    }).then((removeListener) => {
      if (disposed) {
        removeListener();
      } else {
        unlisten = removeListener;
        emitTo("main", "overlay-ready").catch(() => {});
      }
    }).catch(() => {});

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!report) return;

    const generation = ++generationRef.current;
    if (timerRef.current) clearTimeout(timerRef.current);

    const frame = requestAnimationFrame(() => {
      void (async () => {
        const cursor = await cursorPosition().catch(() => null);
        let monitor = cursor
          ? await monitorFromPoint(cursor.x, cursor.y).catch(() => null)
          : null;
        if (!monitor) monitor = await primaryMonitor().catch(() => null);
        if (generationRef.current !== generation) return;

        const measuredHeight = Math.ceil(cardRef.current?.scrollHeight ?? 320) + 2;
        const maxHeight = monitor
          ? Math.max(220, Math.floor(monitor.workArea.size.height / monitor.scaleFactor) - 96)
          : 420;
        const height = Math.min(Math.max(160, measuredHeight), maxHeight);

        await overlayWindow.setSize(new LogicalSize(OVERLAY_WIDTH, height)).catch(() => {});
        if (monitor) {
          // setSize accepts logical units, while monitor work areas and window
          // positions are physical. Read back the real outer size so mixed-DPI
          // displays cannot skew the centering calculation.
          const physicalSize = await overlayWindow.outerSize().catch(() => null);
          const physicalWidth = physicalSize?.width
            ?? Math.round(OVERLAY_WIDTH * monitor.scaleFactor);
          const physicalHeight = physicalSize?.height
            ?? Math.round(height * monitor.scaleFactor);
          const x = monitor.workArea.position.x
            + Math.round((monitor.workArea.size.width - physicalWidth) / 2);
          const y = monitor.workArea.position.y
            + Math.round((monitor.workArea.size.height - physicalHeight) / 2);
          await overlayWindow.setPosition(new PhysicalPosition(x, y)).catch(() => {});
        }
        if (generationRef.current === generation) {
          await overlayWindow.show().catch(() => {});
        }
      })();
    });

    timerRef.current = setTimeout(dismiss, AUTO_HIDE_MS);
    return () => cancelAnimationFrame(frame);
  }, [dismiss, overlayWindow, report]);

  useEffect(() => () => {
    if (timerRef.current) clearTimeout(timerRef.current);
  }, []);

  if (!report) return null;

  const hasFailures = !report.success || report.failed_items.length > 0;
  const hasWarnings = report.warnings.length > 0;
  const hasClosed = report.closed_items.length > 0;
  const accent = hasFailures ? "#f87171" : hasWarnings ? "#fb923c" : "#4bbfc3";

  return (
    <main aria-live="assertive" className="h-screen w-screen p-px" style={{ background: "transparent" }}>
      <div
        ref={cardRef}
        className="flex max-h-full w-full flex-col overflow-hidden rounded-2xl border shadow-2xl"
        style={{
          background: "rgba(22, 24, 29, 0.92)",
          borderColor: "rgba(255,255,255,0.18)",
          backdropFilter: "blur(32px) saturate(145%)",
          boxShadow: "0 24px 70px rgba(0,0,0,0.62), inset 0 1px rgba(255,255,255,0.08)",
        }}
      >
        <header className="flex items-start gap-3 border-b px-5 py-4" style={{ borderColor: "rgba(255,255,255,0.09)" }}>
          <span
            aria-hidden="true"
            className="mt-0.5 grid h-6 w-6 shrink-0 place-items-center rounded-full text-xs font-bold"
            style={{ color: accent, background: `${accent}24` }}
          >
            {hasFailures ? "×" : hasWarnings ? "!" : "✓"}
          </span>
          <div className="min-w-0">
            <h1 className="m-0 text-sm font-semibold text-white">
              {hasFailures
                ? "Restore partially failed"
                : hasWarnings
                  ? "Restore completed with warnings"
                  : "Restore complete"}
            </h1>
            <p className="mt-1 text-xs leading-5" style={{ color: "rgba(255,255,255,0.76)" }}>
              {report.message}
            </p>
          </div>
        </header>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
          {report.failed_items.length > 0 && (
            <ReportSection title="Could not launch" items={report.failed_items} color="#f87171" />
          )}
          {hasWarnings && <ReportSection title="Warnings" items={report.warnings} color="#fb923c" />}
          {hasClosed && (
            <ReportSection title="Closed (not in snapshot)" items={report.closed_items} color="#4bbfc3" />
          )}
        </div>

        <footer className="flex justify-end border-t px-5 py-3" style={{ borderColor: "rgba(255,255,255,0.09)" }}>
          <button
            type="button"
            onClick={dismiss}
            className="cursor-pointer rounded-lg px-4 py-2 text-sm font-medium transition-colors hover:bg-white/15 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-white"
            style={{ color: "rgba(255,255,255,0.92)", background: "rgba(255,255,255,0.14)" }}
          >
            Dismiss
          </button>
        </footer>
      </div>
    </main>
  );
}

function ReportSection({ title, items, color }: { title: string; items: string[]; color: string }) {
  return (
    <section>
      <h2 className="mb-2 text-[11px] font-semibold uppercase tracking-wider" style={{ color }}>
        {title}
      </h2>
      <ul className="m-0 space-y-1.5 p-0">
        {items.map((item, index) => (
          <FailureItem key={`${item}-${index}`} text={item} color={color} />
        ))}
      </ul>
    </section>
  );
}

function FailureItem({ text, color }: { text: string; color: string }) {
  const colonIndex = text.indexOf(": ");
  const name = colonIndex >= 0 ? text.slice(0, colonIndex) : text;
  const reason = colonIndex >= 0 ? text.slice(colonIndex + 2) : null;

  return (
    <li className="flex flex-col gap-0.5 rounded-lg px-3 py-2 text-xs" style={{ background: "rgba(255,255,255,0.09)" }}>
      <span className="font-medium" style={{ color }}>{name}</span>
      {reason && <span style={{ color: "rgba(255,255,255,0.72)" }}>{reason}</span>}
    </li>
  );
}
