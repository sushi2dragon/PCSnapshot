import { TakeSnapshotButton } from "./TakeSnapshotButton";

interface EmptyStateProps {
  onTakeSnapshot: () => void;
}

export function EmptyState({ onTakeSnapshot }: EmptyStateProps) {
  return (
    <div className="flex items-center justify-center min-h-screen">
      <div
        className="flex flex-col items-center gap-5 px-14 py-12 rounded-xl"
        style={{ backgroundColor: "var(--bg-card)" }}
      >
        <p className="text-sm" style={{ color: "var(--text-secondary)" }}>
          No snapshots yet
        </p>
        <TakeSnapshotButton variant="centered" onClick={onTakeSnapshot} />
      </div>
    </div>
  );
}
