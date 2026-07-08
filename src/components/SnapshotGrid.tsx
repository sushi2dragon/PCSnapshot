import { useState, useMemo } from "react";
import type { SnapshotSummary } from "../types/snapshot";
import { SnapshotTile } from "./SnapshotTile";
import { TakeSnapshotButton } from "./TakeSnapshotButton";
import { SettingsMenu } from "./SettingsMenu";

interface SnapshotGridProps {
  snapshots: SnapshotSummary[];
  onRestore: (id: string) => void;
  onDelete: (id: string) => void;
  onRecapture: (id: string) => void;
  onTakeSnapshot: () => void;
  onClearAll: () => void;
  onImport: () => void;
  onHelp: () => void;
  onRefresh: () => void;
  onIgnoreList: () => void;
}

export function SnapshotGrid({
  snapshots,
  onRestore,
  onDelete,
  onRecapture,
  onTakeSnapshot,
  onClearAll,
  onImport,
  onHelp,
  onRefresh,
  onIgnoreList,
}: SnapshotGridProps) {
  const [search, setSearch] = useState("");
  const [refreshing, setRefreshing] = useState(false);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    const sorted = [...snapshots].sort(
      (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
    );
    if (!q) return sorted;
    return sorted.filter((s) => s.name.toLowerCase().includes(q));
  }, [snapshots, search]);

  const handleRefresh = async () => {
    if (refreshing) return; // ignore re-clicks while a refresh is in flight
    setRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setTimeout(() => setRefreshing(false), 600);
    }
  };

  const RefreshButton = (
    <button
      onClick={handleRefresh}
      disabled={refreshing}
      className="flex items-center justify-center rounded-md cursor-pointer transition-colors"
      style={{
        width: 28,
        height: 28,
        backgroundColor: "transparent",
        color: "var(--text-secondary)",
      }}
      onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--bg-tile)")}
      onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "transparent")}
      title="Refresh"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        style={{
          transition: "transform 0.6s ease",
          transform: refreshing ? "rotate(360deg)" : "rotate(0deg)",
        }}
      >
        <polyline points="23 4 23 10 17 10" />
        <polyline points="1 20 1 14 7 14" />
        <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
      </svg>
    </button>
  );

  return (
    <div className="flex items-center justify-center min-h-screen px-8">
      <div
        className="rounded-xl px-8 py-7"
        style={{ backgroundColor: "var(--bg-card)" }}
      >
        {/* Top row: label + search bar */}
        <div className="flex items-center gap-4 mb-5">
          <p className="text-sm font-bold flex-shrink-0" style={{ color: "var(--text-primary)" }}>
            Recent
          </p>
          <div className="relative flex-1" style={{ minWidth: 180 }}>
            <svg
              className="absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none"
              xmlns="http://www.w3.org/2000/svg"
              width="13"
              height="13"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              style={{ color: "var(--text-secondary)" }}
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search snapshots..."
              className="w-full pl-8 pr-3 py-1.5 rounded-md text-xs focus:outline-none"
              style={{
                backgroundColor: "var(--bg-tile)",
                border: "1px solid var(--border-subtle)",
                color: "var(--text-primary)",
              }}
              onFocus={(e) => (e.currentTarget.style.borderColor = "var(--color-accent)")}
              onBlur={(e) => (e.currentTarget.style.borderColor = "var(--border-subtle)")}
            />
            {search && (
              <button
                onClick={() => setSearch("")}
                className="absolute right-2 top-1/2 -translate-y-1/2 cursor-pointer"
                style={{ color: "var(--text-secondary)" }}
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            )}
          </div>
        </div>

        {/* Tiles + divider + buttons */}
        <div className="flex items-start">
          {/* Left: tile grid + refresh pinned to bottom-right of grid area */}
          <div className="flex flex-col" style={{ maxWidth: 560 }}>
            {filtered.length > 0 ? (
              <div className="flex gap-4 flex-wrap">
                {filtered.map((snapshot) => (
                  <SnapshotTile
                    key={snapshot.id}
                    snapshot={snapshot}
                    onRestore={onRestore}
                    onDelete={onDelete}
                    onRecapture={onRecapture}
                  />
                ))}
              </div>
            ) : (
              <div
                className="flex items-center justify-center rounded-lg"
                style={{ width: 300, height: 172, color: "var(--text-secondary)" }}
              >
                <p className="text-xs">No snapshots match "{search}"</p>
              </div>
            )}
            {/* Refresh pinned to bottom-right of tile area */}
            <div className="flex justify-end mt-2">
              {RefreshButton}
            </div>
          </div>

          {/* Vertical divider */}
          <div
            className="mx-7 self-stretch rounded-full flex-shrink-0"
            style={{ width: 1, minHeight: 172, backgroundColor: "var(--border-subtle)" }}
          />

          {/* Right column: Take Snapshot + Settings */}
          <div className="flex-shrink-0 flex flex-col gap-2">
            <TakeSnapshotButton variant="header" onClick={onTakeSnapshot} />
            <SettingsMenu
              onClearAll={onClearAll}
              onImport={onImport}
              onHelp={onHelp}
              onIgnoreList={onIgnoreList}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
