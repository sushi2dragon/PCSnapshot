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

/** Whether the PowerShell directory-capture profile hook is installed. */
export async function terminalHookStatus(): Promise<boolean> {
  return invoke<boolean>("terminal_hook_status");
}

/** Install (enabled=true) or remove the PowerShell directory-capture hook. Returns a status message. */
export async function setTerminalHook(enabled: boolean): Promise<string> {
  return invoke<string>("set_terminal_hook", { enabled });
}
