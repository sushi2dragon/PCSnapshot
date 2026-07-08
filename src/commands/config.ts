import { invoke } from "@tauri-apps/api/core";

export async function getIgnoreList(): Promise<string[]> {
  return invoke<string[]>("get_ignore_list");
}

export async function addToIgnoreList(exeName: string): Promise<void> {
  return invoke<void>("add_to_ignore_list", { exeName });
}

export async function removeFromIgnoreList(exeName: string): Promise<void> {
  return invoke<void>("remove_from_ignore_list", { exeName });
}

export async function getRunningProcesses(): Promise<string[]> {
  return invoke<string[]>("get_running_processes");
}
