import { useState, useRef, useEffect } from "react";
import type { SnapshotSummary } from "../types/snapshot";
import { thumbnailUrl } from "../utils/thumbnail";

interface SnapshotTileProps {
  snapshot: SnapshotSummary;
  onRestore: (id: string) => void;
  onDelete: (id: string) => void;
  onRecapture: (id: string) => void;
}

function formatRelativeTime(timestamp: string): string {
  const diff = Date.now() - new Date(timestamp).getTime();
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return "Just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days} day${days !== 1 ? "s" : ""} ago`;
}

const TILE_W = 130;
const TILE_H = 172;

export function SnapshotTile({ snapshot, onRestore, onDelete, onRecapture }: SnapshotTileProps) {
  const [hovered, setHovered] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const confirmTimer = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (confirmTimer.current !== null) clearTimeout(confirmTimer.current);
    };
  }, []);

  const handleDeleteClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (confirmDelete) {
      onDelete(snapshot.id);
    } else {
      setConfirmDelete(true);
      if (confirmTimer.current !== null) clearTimeout(confirmTimer.current);
      confirmTimer.current = window.setTimeout(() => setConfirmDelete(false), 3000);
    }
  };

  return (
    <div
      className="flex flex-col cursor-pointer"
      style={{ width: TILE_W }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={() => onRestore(snapshot.id)}
    >
      {/* Thumbnail image area */}
      <div
        className="relative rounded-md overflow-hidden flex-shrink-0"
        style={{
          width: TILE_W,
          height: TILE_H,
          backgroundColor: hovered ? "var(--bg-tile-hover)" : "var(--bg-tile)",
          transition: "background-color 0.12s",
        }}
      >
        {snapshot.thumbnail_path ? (
          <img
            src={thumbnailUrl(snapshot.thumbnail_path, snapshot.timestamp)}
            alt={snapshot.name}
            className="w-full h-full object-cover"
          />
        ) : (
          <div
            className="w-full h-full"
            style={{
              background: "radial-gradient(ellipse at 40% 35%, #36363b 0%, #28282c 100%)",
            }}
          />
        )}

        {/* Hover overlay */}
        <div
          className="absolute inset-0 flex items-center justify-center"
          style={{
            backgroundColor: "rgba(0,0,0,0.52)",
            opacity: hovered ? 1 : 0,
            transition: "opacity 0.12s",
          }}
        >
          <button
            onClick={(e) => { e.stopPropagation(); onRestore(snapshot.id); }}
            className="px-4 py-1.5 text-xs font-semibold text-white rounded-md cursor-pointer"
            style={{ backgroundColor: "var(--color-accent)" }}
          >
            Restore
          </button>
        </div>

        {/* Recapture button */}
        <button
          onClick={(e) => { e.stopPropagation(); onRecapture(snapshot.id); }}
          className="absolute top-1.5 left-1.5 p-1 rounded cursor-pointer"
          style={{
            opacity: hovered ? 1 : 0,
            transition: "opacity 0.12s",
            backgroundColor: "rgba(0,0,0,0.45)",
            color: "rgba(255,255,255,0.75)",
          }}
          title="Recapture"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="23 4 23 10 17 10" />
            <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
          </svg>
        </button>

        {/* Delete button */}
        <button
          onClick={handleDeleteClick}
          className="absolute top-1.5 right-1.5 p-1 rounded cursor-pointer"
          style={{
            opacity: hovered ? 1 : 0,
            transition: "opacity 0.12s",
            backgroundColor: confirmDelete ? "rgba(220,38,38,0.85)" : "rgba(0,0,0,0.45)",
            color: "rgba(255,255,255,0.75)",
          }}
          title={confirmDelete ? "Click again to confirm" : "Delete"}
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="3 6 5 6 21 6" />
            <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
          </svg>
        </button>
      </div>

      {/* Text below */}
      <div className="mt-2 px-0.5">
        <p className="text-sm font-semibold truncate leading-snug" style={{ color: "var(--text-primary)" }}>
          {snapshot.name}
        </p>
        <p className="text-xs mt-0.5 leading-snug" style={{ color: "var(--text-secondary)" }}>
          Edited {formatRelativeTime(snapshot.timestamp)}
        </p>
      </div>
    </div>
  );
}
