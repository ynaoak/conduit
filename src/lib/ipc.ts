import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  PinnedItem,
  SearchResult,
  WorkflowManifest,
} from "./types";

export async function searchQuery(query: string): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("query", { query });
}

export async function executeAction(
  pluginId: string,
  resultId: string,
  actionId?: string,
): Promise<void> {
  return invoke("execute_action", { pluginId, resultId, actionId });
}

export async function getConfig(): Promise<AppConfig> {
  return invoke<AppConfig>("get_config");
}

export async function saveConfig(config: AppConfig): Promise<void> {
  return invoke("save_config", { config });
}

export async function getConfigPath(): Promise<string> {
  return invoke<string>("get_config_path");
}

/** Open config.json in the OS default editor */
export async function openConfigFile(): Promise<void> {
  return invoke("open_config_file");
}

/** Restart the app (applies startup-only settings like hotkey / double_tap) */
export async function restartApp(): Promise<void> {
  return invoke("restart_app");
}

/** Pinned favorites */
export async function listPins(): Promise<PinnedItem[]> {
  return invoke<PinnedItem[]>("list_pins");
}

export async function addPin(item: PinnedItem): Promise<void> {
  return invoke("add_pin", { item });
}

export async function removePin(resultId: string): Promise<void> {
  return invoke("remove_pin", { resultId });
}

export async function movePin(
  resultId: string,
  newIndex: number,
): Promise<void> {
  return invoke("move_pin", { resultId, newIndex });
}

/** Pin the window: while pinned it ignores hide-on-blur */
export async function setPinned(pinned: boolean): Promise<void> {
  return invoke("set_pinned", { pinned });
}

/** Import a workflow zip; resolves to the workflow's display name */
export async function importWorkflow(path: string): Promise<string> {
  return invoke<string>("import_workflow", { path });
}

/** List installed workflow packages */
export async function listWorkflows(): Promise<WorkflowManifest[]> {
  return invoke<WorkflowManifest[]>("list_workflows");
}

/** Enumerate a plugin's registered entries (browsing UI, not search) */
export async function browsePlugin(pluginId: string): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("browse_plugin", { pluginId });
}
