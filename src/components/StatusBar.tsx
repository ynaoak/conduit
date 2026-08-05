import type { CSSProperties } from "react";
import { chordLabel } from "../lib/keys";
import type { Keybindings } from "../lib/types";

interface StatusBarProps {
  pluginName?: string;
  keybindings: Keybindings;
}

const containerStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  padding: "0 20px",
  height: "32px",
  borderTop: "1px solid var(--border-color)",
  fontSize: "11px",
  color: "var(--text-secondary)",
  flexShrink: 0,
  // The whole footer doubles as a window drag area
  cursor: "grab",
};

/** Children must not swallow mousedown: the drag region check only matches
 *  the element that carries data-tauri-drag-region itself. */
const passthroughStyle: CSSProperties = {
  pointerEvents: "none",
};

const PLUGIN_DISPLAY_NAMES: Record<string, string> = {
  "conduit.app-launcher": "アプリケーション",
  "conduit.calculator": "電卓",
  "conduit.clipboard-history": "クリップボード履歴",
  "conduit.system-commands": "システム",
  "conduit.web-search": "Web 検索",
  "conduit.workflows": "ワークフロー",
  "conduit.file-search": "ファイル検索",
  "conduit.window-switcher": "ウィンドウ",
  "conduit.process-monitor": "プロセス",
};

export function StatusBar({ pluginName, keybindings }: StatusBarProps) {
  const displayName = pluginName
    ? PLUGIN_DISPLAY_NAMES[pluginName] ?? pluginName
    : "Conduit";

  // Hints follow the configured keybindings (first chord of each action)
  const moveLabel = `${chordLabel(keybindings.move_up)}${chordLabel(keybindings.move_down)}`;

  return (
    <div style={containerStyle} data-tauri-drag-region>
      <span style={passthroughStyle}>{displayName}</span>
      <span style={passthroughStyle}>
        <kbd style={kbdStyle}>{moveLabel}</kbd> 移動{" "}
        <kbd style={kbdStyle}>{chordLabel(keybindings.execute)}</kbd> 実行{" "}
        <kbd style={kbdStyle}>{chordLabel(keybindings.action_panel)}</kbd>{" "}
        アクション{" "}
        <kbd style={kbdStyle}>{chordLabel(keybindings.toggle_pin)}</kbd> ピン{" "}
        <kbd style={kbdStyle}>{chordLabel(keybindings.close)}</kbd> 閉じる
      </span>
    </div>
  );
}

const kbdStyle: CSSProperties = {
  background: "var(--bg-secondary)",
  padding: "1px 5px",
  borderRadius: "3px",
  fontSize: "10px",
  marginRight: "2px",
};
