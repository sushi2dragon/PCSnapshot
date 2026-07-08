import { useEffect } from "react";

interface RecaptureConfirmModalProps {
  snapshotName: string | null;
  onConfirm: () => void;
  onCancel: () => void;
}

export function RecaptureConfirmModal({
  snapshotName,
  onConfirm,
  onCancel,
}: RecaptureConfirmModalProps) {
  // Keyboard: Esc = cancel, Enter = confirm.
  useEffect(() => {
    if (!snapshotName) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
      if (e.key === "Enter") {
        // Stop a focused button's default Enter activation from firing a
        // second onConfirm/onCancel in the same keystroke.
        e.preventDefault();
        onConfirm();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [snapshotName, onConfirm, onCancel]);

  if (!snapshotName) return null;

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center"
      style={{ backgroundColor: "rgba(0,0,0,0.6)" }}
      onClick={onCancel}
    >
      <div
        className="rounded-xl shadow-2xl p-6 w-80"
        style={{ backgroundColor: "var(--bg-card)" }}
        onClick={(e) => e.stopPropagation()}
      >
        <h2
          className="text-sm font-semibold mb-2"
          style={{ color: "var(--text-primary)" }}
        >
          Recapture snapshot
        </h2>
        <p className="text-xs mb-4" style={{ color: "var(--text-secondary)" }}>
          Overwrite <strong style={{ color: "var(--text-primary)" }}>{snapshotName}</strong> with
          the current desktop state? The previous capture data will be replaced.
        </p>
        <div className="flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="px-4 py-1.5 text-xs rounded-md cursor-pointer transition-colors"
            style={{
              color: "var(--text-secondary)",
              backgroundColor: "transparent",
            }}
            onMouseEnter={(e) =>
              (e.currentTarget.style.backgroundColor = "var(--bg-tile)")
            }
            onMouseLeave={(e) =>
              (e.currentTarget.style.backgroundColor = "transparent")
            }
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            className="px-4 py-1.5 text-xs font-semibold text-white rounded-md cursor-pointer"
            style={{ backgroundColor: "var(--color-accent)" }}
            onMouseEnter={(e) =>
              (e.currentTarget.style.backgroundColor = "var(--color-accent-hover)")
            }
            onMouseLeave={(e) =>
              (e.currentTarget.style.backgroundColor = "var(--color-accent)")
            }
          >
            Recapture
          </button>
        </div>
      </div>
    </div>
  );
}
