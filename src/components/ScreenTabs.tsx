import type { CSSProperties } from "react";
import type { ManageTab } from "./ManageView";

export type Screen = "search" | ManageTab;

interface ScreenTabsProps {
  screen: Screen;
  onSelect: (screen: Screen) => void;
}

const TABS: { screen: Screen; icon: string; label: string }[] = [
  { screen: "search", icon: "🔍", label: "検索" },
  { screen: "pins", icon: "📌", label: "ピン留め" },
  { screen: "workflows", icon: "📦", label: "ワークフロー" },
  { screen: "launcher", icon: "🚀", label: "ランチャー" },
  { screen: "keybindings", icon: "⚙️", label: "キー設定" },
];

const rowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "2px",
  flexShrink: 0,
  marginLeft: "8px",
};

function tabStyle(active: boolean): CSSProperties {
  return {
    background: active ? "var(--bg-selected)" : "transparent",
    border: "none",
    borderRadius: "8px",
    fontSize: "15px",
    lineHeight: 1,
    padding: "6px 9px",
    cursor: "pointer",
    opacity: active ? 1 : 0.5,
    transition: "opacity var(--transition-fast), background var(--transition-fast)",
  };
}

/** Persistent screen switcher shown next to the search input — makes the
 *  Tab-cycled screens (workflows / launcher / key settings) discoverable */
export function ScreenTabs({ screen, onSelect }: ScreenTabsProps) {
  return (
    <div style={rowStyle} role="tablist">
      {TABS.map(({ screen: tab, icon, label }) => (
        <button
          key={tab}
          role="tab"
          aria-selected={screen === tab}
          style={tabStyle(screen === tab)}
          onClick={() => onSelect(tab)}
          title={`${label} (Tab で循環)`}
          onMouseEnter={(e) => {
            if (screen !== tab)
              (e.currentTarget as HTMLButtonElement).style.opacity = "0.85";
          }}
          onMouseLeave={(e) => {
            if (screen !== tab)
              (e.currentTarget as HTMLButtonElement).style.opacity = "0.5";
          }}
        >
          {icon}
        </button>
      ))}
    </div>
  );
}
