import { useEffect, useState } from "react";

interface RestoreConfirmModalProps {
  /** Name of the snapshot about to be restored; null = modal closed. */
  snapshotName: string | null;
  /** null while we're still checking; true/false once known. */
  currentStateSaved: boolean | null;
  onConfirm: (closeOthers: boolean) => void;
  onSaveFirst: () => void;
  onCancel: () => void;
}

export function RestoreConfirmModal({
  snapshotName,
  currentStateSaved,
  onConfirm,
  onSaveFirst,
  onCancel,
}: RestoreConfirmModalProps) {
  // The parent remounts this modal (via key) per snapshot, so the toggle
  // starts fresh each time without needing setState in an effect.
  const [closeOthers, setCloseOthers] = useState(true);

  // Keyboard: Esc = cancel, Enter = restore (matches the in-app Help text).
  useEffect(() => {
    if (snapshotName === null) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
      if (e.key === "Enter") {
        // Stop a focused button's default Enter activation from firing a
        // second onConfirm/onCancel in the same keystroke.
        e.preventDefault();
        onConfirm(closeOthers);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [snapshotName, closeOthers, onConfirm, onCancel]);

  if (snapshotName === null) return null;

  const unsaved = currentStateSaved === false;
  const checking = currentStateSaved === null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ backgroundColor: "rgba(0,0,0,0.6)" }}
      onClick={onCancel}
    >
      <div
        className="w-full max-w-md rounded-xl shadow-2xl overflow-hidden"
        style={{ backgroundColor: "var(--bg-card, #252528)", border: "1px solid rgba(255,255,255,0.08)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="px-6 pt-5 pb-4">
          <h2 className="text-sm font-semibold text-white">
            Restore “{snapshotName}”?
          </h2>

          {/* Unsaved-state warning */}
          {unsaved && (
            <div
              className="mt-3 flex items-start gap-2.5 rounded-lg px-3 py-2.5"
              style={{ backgroundColor: "rgba(251,146,60,0.12)", border: "1px solid rgba(251,146,60,0.25)" }}
            >
              <span className="text-base leading-none mt-0.5" style={{ color: "#fb923c" }}>!</span>
              <p className="text-xs leading-relaxed" style={{ color: "#fbbf94" }}>
                Your current desktop doesn’t match any saved snapshot. If you close
                windows that aren’t part of this one, that arrangement will be lost.
              </p>
            </div>
          )}
          {currentStateSaved === true && (
            <p className="mt-2 text-xs" style={{ color: "rgba(255,255,255,0.45)" }}>
              Your current desktop is already saved in a snapshot.
            </p>
          )}
          {checking && (
            <p className="mt-2 text-xs" style={{ color: "rgba(255,255,255,0.35)" }}>
              Checking your current desktop…
            </p>
          )}
        </div>

        {/* Clean-restore toggle */}
        <button
          onClick={() => setCloseOthers((v) => !v)}
          className="w-full px-6 py-3 flex items-start gap-3 text-left transition-colors"
          style={{ backgroundColor: "rgba(255,255,255,0.03)" }}
        >
          <span
            className="mt-0.5 flex-shrink-0 w-4 h-4 rounded flex items-center justify-center text-[10px] font-bold"
            style={{
              backgroundColor: closeOthers ? "var(--color-accent)" : "transparent",
              border: closeOthers ? "none" : "1.5px solid rgba(255,255,255,0.3)",
              color: "#fff",
            }}
          >
            {closeOthers ? "✓" : ""}
          </span>
          <span className="flex flex-col gap-0.5">
            <span className="text-xs font-medium text-white">
              Close apps that aren’t part of this snapshot
            </span>
            <span className="text-[11px]" style={{ color: "rgba(255,255,255,0.45)" }}>
              Leaves your desktop matching the snapshot exactly. Apps with unsaved
              work will ask before closing.
            </span>
          </span>
        </button>

        {/* Footer */}
        <div
          className="px-6 py-4 flex items-center justify-end gap-2 border-t"
          style={{ borderColor: "rgba(255,255,255,0.08)" }}
        >
          {unsaved && (
            <button
              onClick={onSaveFirst}
              className="px-3.5 py-1.5 rounded-md text-sm font-medium transition-colors mr-auto"
              style={{ backgroundColor: "rgba(59,127,235,0.22)", color: "#3B7FEB" }}
            >
              Save current first
            </button>
          )}
          <button
            onClick={onCancel}
            className="px-3.5 py-1.5 rounded-md text-sm font-medium transition-colors"
            style={{ backgroundColor: "rgba(255,255,255,0.08)", color: "rgba(255,255,255,0.7)" }}
          >
            Cancel
          </button>
          <button
            onClick={() => onConfirm(closeOthers)}
            className="px-4 py-1.5 rounded-md text-sm font-semibold transition-colors"
            style={{ backgroundColor: "#3B7FEB", color: "#fff" }}
          >
            Restore
          </button>
        </div>
      </div>
    </div>
  );
}
