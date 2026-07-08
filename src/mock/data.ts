import type { SnapshotSummary } from "../types/snapshot";

export const mockSnapshots: SnapshotSummary[] = [
  {
    id: "snap_001",
    name: "Dev Workflow",
    timestamp: new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString(),
    thumbnail_path: "",
    warning_count: 0,
  },
  {
    id: "snap_002",
    name: "Research Session",
    timestamp: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(),
    thumbnail_path: "",
    warning_count: 1,
  },
  {
    id: "snap_003",
    name: "Design Review",
    timestamp: new Date(Date.now() - 3 * 24 * 60 * 60 * 1000).toISOString(),
    thumbnail_path: "",
    warning_count: 0,
  },
];
