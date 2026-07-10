import { useState, useCallback, useRef, useEffect } from "react";
import { useSnapshots } from "./hooks/useSnapshots";
import { EmptyState } from "./components/EmptyState";
import { SnapshotGrid } from "./components/SnapshotGrid";
import { NamePromptModal } from "./components/NamePromptModal";
import { Toast } from "./components/Toast";
import { RestoreReportModal } from "./components/RestoreReportModal";
import { RestoreConfirmModal } from "./components/RestoreConfirmModal";
import { IgnoreListModal } from "./components/IgnoreListModal";
import { RecaptureConfirmModal } from "./components/RecaptureConfirmModal";
import { isCurrentStateSaved, clearAllSnapshots } from "./commands/snapshots";
import { terminalHookStatus, setTerminalHook } from "./commands/config";
import type { RestoreResult } from "./types/snapshot";

function App() {
  const { snapshots, loading, capture, recapture, restore, remove, refresh } = useSnapshots();
  const [modalOpen, setModalOpen] = useState(false);
  const [toast, setToast] = useState<{ message: string; type: "success" | "warning" } | null>(null);
  const [restoreReport, setRestoreReport] = useState<RestoreResult | null>(null);
  // Restore confirmation flow
  const [confirmRestore, setConfirmRestore] = useState<{ id: string; name: string } | null>(null);
  const [currentStateSaved, setCurrentStateSaved] = useState<boolean | null>(null);
  // When set, the next capture is a "save current desktop, then restore this id" flow.
  const [saveFirstPendingId, setSaveFirstPendingId] = useState<string | null>(null);
  const [showIgnoreList, setShowIgnoreList] = useState(false);
  const [confirmRecapture, setConfirmRecapture] = useState<{ id: string; name: string } | null>(null);
  const [terminalHookEnabled, setTerminalHookEnabled] = useState(false);

  useEffect(() => {
    terminalHookStatus().then(setTerminalHookEnabled).catch(() => {});
  }, []);

  // Next free "Snapshot NN" number — derived from existing names (not the array
  // length) so deleting snapshots never produces a duplicate default name.
  const nextNumber =
    snapshots.reduce((max, s) => {
      const m = /^Snapshot (\d+)$/.exec(s.name);
      return m ? Math.max(max, parseInt(m[1], 10)) : max;
    }, 0) + 1;
  const defaultName = `Snapshot ${String(nextNumber).padStart(2, "0")}`;

  const handleTakeSnapshot = useCallback(() => setModalOpen(true), []);

  const handleConfirmCapture = useCallback(
    async (name: string) => {
      setModalOpen(false);
      try {
        const warnings = await capture(name);

        // "Save current first" flow: after capturing the live desktop, return to the
        // restore confirmation for the snapshot the user originally picked.
        if (saveFirstPendingId) {
          const id = saveFirstPendingId;
          setSaveFirstPendingId(null);
          const snap = snapshots.find((s) => s.id === id);
          setCurrentStateSaved(true); // we just saved it
          setConfirmRestore({ id, name: snap?.name ?? "snapshot" });
          return;
        }

        setToast(
          warnings.length > 0
            ? { message: `Snapshot saved with ${warnings.length} warning(s)`, type: "warning" }
            : { message: "Snapshot captured", type: "success" }
        );
      } catch (e) {
        setSaveFirstPendingId(null);
        setToast({ message: `Capture failed: ${e}`, type: "warning" });
      }
    },
    [capture, saveFirstPendingId, snapshots]
  );

  // Guards the async "is current state saved?" check: only the latest request may
  // write its result, so a stale response never clobbers a newer dialog's state.
  const savedCheckToken = useRef(0);

  // Step 1: clicking a tile opens the confirmation dialog and kicks off the
  // "is the current desktop already saved?" check.
  const handleRestore = useCallback(
    (id: string) => {
      const snap = snapshots.find((s) => s.id === id);
      setConfirmRestore({ id, name: snap?.name ?? "snapshot" });
      setCurrentStateSaved(null);
      const token = ++savedCheckToken.current;
      isCurrentStateSaved()
        .then((saved) => {
          if (savedCheckToken.current === token) setCurrentStateSaved(saved);
        })
        .catch(() => {
          // unknown → treat as unsaved (safer)
          if (savedCheckToken.current === token) setCurrentStateSaved(false);
        });
    },
    [snapshots]
  );

  // Step 2: user confirmed — run the actual restore.
  const handleConfirmRestore = useCallback(
    async (closeOthers: boolean) => {
      if (!confirmRestore) return;
      const id = confirmRestore.id;
      setConfirmRestore(null);
      try {
        const result = await restore(id, closeOthers);
        const hasDetail =
          result.failed_items.length > 0 ||
          result.warnings.length > 0 ||
          result.closed_items.length > 0;
        if (!result.success || hasDetail) {
          setRestoreReport(result);
        } else {
          setToast({ message: result.message, type: "success" });
        }
      } catch (e) {
        setToast({ message: `Restore failed: ${e}`, type: "warning" });
      }
    },
    [confirmRestore, restore]
  );

  // "Save current first": stash the target, then open the capture name prompt.
  const handleSaveFirst = useCallback(() => {
    if (!confirmRestore) return;
    setSaveFirstPendingId(confirmRestore.id);
    setConfirmRestore(null);
    setModalOpen(true);
  }, [confirmRestore]);

  const handleRecapture = useCallback(
    (id: string) => {
      const snap = snapshots.find((s) => s.id === id);
      setConfirmRecapture({ id, name: snap?.name ?? "snapshot" });
    },
    [snapshots]
  );

  const handleConfirmRecapture = useCallback(async () => {
    if (!confirmRecapture) return;
    const id = confirmRecapture.id;
    setConfirmRecapture(null);
    try {
      const warnings = await recapture(id);
      setToast(
        warnings.length > 0
          ? { message: `Snapshot updated with ${warnings.length} warning(s)`, type: "warning" }
          : { message: "Snapshot updated", type: "success" }
      );
    } catch (e) {
      setToast({ message: `Recapture failed: ${e}`, type: "warning" });
    }
  }, [confirmRecapture, recapture]);

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await remove(id);
        setToast({ message: "Snapshot deleted", type: "success" });
      } catch (e) {
        setToast({ message: `Delete failed: ${e}`, type: "warning" });
      }
    },
    [remove]
  );

  const handleClearAll = useCallback(async () => {
    try {
      await clearAllSnapshots();
      await refresh();
      setToast({ message: "All snapshots deleted", type: "success" });
    } catch (e) {
      setToast({ message: `Clear all failed: ${e}`, type: "warning" });
    }
  }, [refresh]);

  const handleImport = useCallback(() => {
    // TODO: open file picker via Tauri dialog
    setToast({ message: "Import — not yet implemented", type: "warning" });
  }, []);

  const handleHelp = useCallback(() => {
    setToast({ message: "Keyboard: Enter = restore  ·  Del = delete  ·  Esc = cancel", type: "success" });
  }, []);

  const handleRefresh = useCallback(async () => {
    await refresh();
    setToast({ message: "Refreshed", type: "success" });
  }, [refresh]);

  const handleToggleTerminalHook = useCallback(async () => {
    const next = !terminalHookEnabled;
    try {
      const message = await setTerminalHook(next);
      setTerminalHookEnabled(next);
      setToast({ message, type: "success" });
    } catch (e) {
      setToast({ message: `Terminal capture: ${e}`, type: "warning" });
    }
  }, [terminalHookEnabled]);

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-screen" style={{ backgroundColor: "var(--bg-base)" }}>
        <div
          className="w-7 h-7 rounded-full border-2 animate-spin"
          style={{ borderColor: "var(--border-subtle)", borderTopColor: "var(--color-accent)" }}
        />
      </div>
    );
  }

  return (
    <div className="min-h-screen" style={{ backgroundColor: "var(--bg-base)" }}>
      {snapshots.length === 0 ? (
        <EmptyState onTakeSnapshot={handleTakeSnapshot} />
      ) : (
        <SnapshotGrid
          snapshots={snapshots}
          onRestore={handleRestore}
          onDelete={handleDelete}
          onRecapture={handleRecapture}
          onTakeSnapshot={handleTakeSnapshot}
          onClearAll={handleClearAll}
          onImport={handleImport}
          onHelp={handleHelp}
          onRefresh={handleRefresh}
          onIgnoreList={() => setShowIgnoreList(true)}
          onToggleTerminalHook={handleToggleTerminalHook}
          terminalHookEnabled={terminalHookEnabled}
        />
      )}

      <NamePromptModal
        key={modalOpen ? "open" : "closed"}
        isOpen={modalOpen}
        defaultName={defaultName}
        onConfirm={handleConfirmCapture}
        onCancel={() => {
          setModalOpen(false);
          setSaveFirstPendingId(null);
        }}
      />

      <RestoreConfirmModal
        key={confirmRestore?.id ?? "closed"}
        snapshotName={confirmRestore?.name ?? null}
        currentStateSaved={currentStateSaved}
        onConfirm={handleConfirmRestore}
        onSaveFirst={handleSaveFirst}
        onCancel={() => setConfirmRestore(null)}
      />

      <Toast
        message={toast?.message ?? null}
        type={toast?.type ?? "success"}
        onDismiss={() => setToast(null)}
      />

      <RestoreReportModal
        result={restoreReport}
        onDismiss={() => setRestoreReport(null)}
      />

      <RecaptureConfirmModal
        snapshotName={confirmRecapture?.name ?? null}
        onConfirm={handleConfirmRecapture}
        onCancel={() => setConfirmRecapture(null)}
      />

      <IgnoreListModal
        key={showIgnoreList ? "open" : "closed"}
        isOpen={showIgnoreList}
        onClose={() => setShowIgnoreList(false)}
      />
    </div>
  );
}

export default App;
