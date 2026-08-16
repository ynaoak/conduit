import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  PinnedItem,
  SearchResult,
  WorkflowListing,
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

/** The language the backend renders in — authoritative for `"system"`,
 *  where the webview and the OS locale can disagree */
export async function resolvedLanguage(): Promise<string> {
  return invoke<string>("resolved_language");
}

/** "download" (self-updating) or "store" (updated through the Store) */
export async function releaseChannel(): Promise<string> {
  return invoke<string>("release_channel");
}

/** The summon chord, in this platform's key names ("Option+Space" on macOS) */
export async function summonHotkey(): Promise<string> {
  return invoke<string>("summon_hotkey");
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

/** Hide the launcher via the Rust side: also returns focus to the window
 *  it was summoned over, so the Ctrl double-tap keeps working afterwards */
export async function hideWindow(): Promise<void> {
  return invoke("hide_window");
}

/** Report the appearance the launcher is actually rendering, so tool
 *  windows (which have no IPC) can be opened matching it */
export async function setResolvedTheme(theme: {
  mode: "dark" | "light";
  colors: Record<string, string>;
}): Promise<void> {
  return invoke("set_resolved_theme", { theme });
}

/** Hide after an action ran. Respects the pin: a pinned launcher stays
 *  open (Esc and the close button still hide it) */
export async function autoHideWindow(): Promise<void> {
  return invoke("auto_hide_window");
}

/** Pin the window: while pinned it never hides on its own */
export async function setPinned(pinned: boolean): Promise<void> {
  return invoke("set_pinned", { pinned });
}

/** Import a workflow zip; resolves to the workflow's display name */
export async function importWorkflow(path: string): Promise<string> {
  return invoke<string>("import_workflow", { path });
}

/** List installed workflow packages with their manage-view state */
export async function listWorkflows(): Promise<WorkflowListing[]> {
  return invoke<WorkflowListing[]>("list_workflows");
}

/** Enable / disable a workflow package (persisted in config.json) */
export async function setWorkflowEnabled(
  id: string,
  enabled: boolean,
): Promise<void> {
  return invoke("set_workflow_enabled", { id, enabled });
}

/** Delete an installed workflow package (built-ins are refused) */
/** Write an installed package out as an importable zip. Returns the path. */
export async function exportWorkflow(id: string, dest: string): Promise<string> {
  return invoke<string>("export_workflow", { id, dest });
}

export async function deleteWorkflow(id: string): Promise<void> {
  return invoke("delete_workflow", { id });
}

/** Enumerate a plugin's registered entries (browsing UI, not search) */
export async function browsePlugin(pluginId: string): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("browse_plugin", { pluginId });
}

/** A screenshot or recording kept in the app's own history
 *  (see src-tauri/src/capture_history.rs) */
export interface Capture {
  id: string;
  kind: "image" | "video";
  file: string;
  thumb: string;
  width: number;
  height: number;
  created_ms: number;
  bytes: number;
  seconds: number;
}

/** What the selection surface draws: the frozen screen, and whether the
 *  rectangle is going to be recorded rather than saved as a still */
export interface CaptureSurface {
  image: string;
  recording: boolean;
  /** Which monitor this surface covers, and how many there are */
  index: number;
  screens: number;
}

/** A monitor, as the backend orders them: left to right */
export interface CaptureScreen {
  index: number;
  bounds: { x: number; y: number; width: number; height: number };
  scale: number;
}

/** A selection, as fractions of the frozen frame — the surface works in
 *  CSS pixels and the frame is in physical ones, so fractions are the one
 *  thing that means the same on both sides */
export interface CaptureSelection {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Take a capture. `screen` names one monitor for a full-screen shot;
 *  without it a full-screen shot is every monitor in one image.
 *  Resolves to null when the user cancelled the selection, and when a
 *  recording started (it files itself on stop). */
export async function takeCapture(
  mode: "region" | "screen" | "record",
  screen?: number,
): Promise<Capture | null> {
  return invoke<Capture | null>("take_capture", { mode, screen });
}

export async function captureScreens(): Promise<CaptureScreen[]> {
  return invoke<CaptureScreen[]>("capture_screens");
}

/** The selection surface's own commands; only that window may call them */
export async function captureFrame(): Promise<CaptureSurface> {
  return invoke<CaptureSurface>("capture_frame");
}

export async function captureSurfaceReady(): Promise<void> {
  return invoke("capture_surface_ready");
}

/** The pointer moved onto this surface — give it the keyboard, so Escape
 *  works on the monitor the user is actually on */
export async function captureSurfaceFocus(): Promise<void> {
  return invoke("capture_surface_focus");
}

export async function captureSelected(
  selection: CaptureSelection | null,
): Promise<void> {
  return invoke("capture_selected", { selection });
}

export async function stopRecording(): Promise<Capture> {
  return invoke<Capture>("stop_recording");
}

export async function recordingState(): Promise<{
  running: boolean;
  max_seconds: number;
}> {
  return invoke("recording_state");
}
