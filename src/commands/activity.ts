import { invoke } from "@tauri-apps/api/core";
import type { ActivityEvent } from "../types/snapshot";
export const listActivity = (limit = 50) => invoke<ActivityEvent[]>("list_activity", { limit });
