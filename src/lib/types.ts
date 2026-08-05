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

export interface WebAppDef {
  name: string;
  url: string;
  keyword: string | null;
  icon: string | null;
  description: string | null;
}

export interface WorkflowManifest {
  id: string;
  name: string;
  description: string;
  icon: string | null;
  apps: WebAppDef[];
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
    bg_primary: string;
    bg_secondary: string;
    bg_hover: string;
    bg_selected: string;
    text_primary: string;
    text_secondary: string;
    text_accent: string;
    border_color: string;
    border_radius: string;
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
}
