export interface SearchResult {
  id: string;
  plugin_id: string;
  title: string;
  subtitle: string | null;
  icon: ResultIcon;
  score: number;
  actions: Action[];
  /** Char indices of `title` that matched the query (for highlighting) */
  match_indices?: number[];
}

export type ResultIcon =
  | { type: "Svg"; value: string }
  | { type: "Named"; value: string }
  | { type: "Emoji"; value: string }
  | { type: "Base64"; value: string };

export interface Action {
  id: string;
  title: string;
  shortcut: string | null;
}

/** A workflow package as returned by list_workflows: manifest content
 *  plus manage-view state, with per-app result ids ready for
 *  execute_action */
export interface WorkflowListing {
  id: string;
  name: string;
  description: string;
  icon: string | null;
  /** Ships with the binary: cannot be deleted, only disabled */
  builtin: boolean;
  disabled: boolean;
  apps: AppListing[];
}

export interface AppListing {
  name: string;
  icon: string | null;
  description: string | null;
  keyword: string | null;
  url: string | null;
  html: string | null;
  /** Feed into executeAction("conduit.workflows", result_id, "open") */
  result_id: string;
}

/** A pinned favorite — carries enough to rebuild the original SearchResult */
export interface PinnedItem {
  plugin_id: string;
  result_id: string;
  title: string;
  subtitle: string | null;
  icon: ResultIcon;
  actions: Action[];
}

export interface Keybindings {
  move_up: string[];
  move_down: string[];
  execute: string[];
  action_panel: string[];
  close: string[];
  toggle_pin: string[];
  /** Toggle the manage view (workflows / launcher / key settings) */
  manage_view: string[];
  /** Cycle tabs inside the manage view */
  tab_next: string[];
  tab_prev: string[];
}

export interface AppConfig {
  hotkey: {
    modifier: string;
    key: string;
    /** Double-tap activation key: "Ctrl" | "Alt" | "Shift" | "Win" | "" (disabled) */
    double_tap: string;
    double_tap_interval_ms: number;
  };
  keybindings: Keybindings;
  theme: {
    /** "system" (follow the OS) | "dark" | "light" */
    mode: string;
    /** Per-slot overrides on top of the built-in palettes */
    bg_primary?: string | null;
    bg_secondary?: string | null;
    bg_hover?: string | null;
    bg_selected?: string | null;
    text_primary?: string | null;
    text_secondary?: string | null;
    text_accent?: string | null;
    border_color?: string | null;
    border_radius?: string | null;
  };
  plugins: {
    enabled: Record<string, boolean>;
  };
  search: {
    max_results: number;
    web_search_engine: string;
  };
  window: {
    hide_on_blur: boolean;
  };
  workflows?: {
    /** Package ids excluded from search and launching */
    disabled: string[];
  };
  /** UI language: a code shipped in locales/ or "system" */
  language: string;
  /** Check GitHub Releases for a signed update shortly after launch */
  auto_update_check?: boolean;
}
