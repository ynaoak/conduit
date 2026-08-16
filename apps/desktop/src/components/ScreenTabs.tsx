import type { CSSProperties } from "react";
import type { ManageTab } from "./ManageView";
import { MaterialIcon } from "./MaterialIcon";
import { useStrings } from "../lib/i18n";

export type Screen = "search" | ManageTab;

interface ScreenTabsProps {
  screen: Screen;
  onSelect: (screen: Screen) => void;
}

/** label is a key in the `tabs` locale section */
const TABS: { screen: Screen; icon: string; label: string }[] = [
  { screen: "search", icon: "search", label: "search" },
  { screen: "pins", icon: "push_pin", label: "pins" },
  { screen: "workflows", icon: "deployed_code", label: "workflows" },
  { screen: "launcher", icon: "rocket_launch", label: "launcher" },
  { screen: "keybindings", icon: "keyboard", label: "keybindings" },
  { screen: "about", icon: "info", label: "about" },
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
    display: "inline-flex",
    alignItems: "center",
    background: active ? "var(--bg-selected)" : "transparent",
    border: "none",
    // M3 navigation marks the active item with a pill indicator
    borderRadius: "999px",
    color: active ? "var(--text-primary)" : "var(--text-secondary)",
    lineHeight: 1,
    padding: "6px 9px",
    cursor: "pointer",
    opacity: active ? 1 : 0.6,
    transition: "opacity var(--transition-fast), background var(--transition-fast)",
  };
}

/** Persistent screen switcher shown next to the search input — makes the
 *  Tab-cycled screens (workflows / launcher / key settings) discoverable */
export function ScreenTabs({ screen, onSelect }: ScreenTabsProps) {
  const s = useStrings();
  return (
    <div style={rowStyle} role="tablist">
      {TABS.map(({ screen: tab, icon, label }) => (
        <button
          key={tab}
          role="tab"
          aria-selected={screen === tab}
          style={tabStyle(screen === tab)}
          onClick={() => onSelect(tab)}
          title={`${s("tabs", label)} (${s("tabs", "cycle_hint")})`}
          onMouseEnter={(e) => {
            if (screen !== tab)
              (e.currentTarget as HTMLButtonElement).style.opacity = "0.85";
          }}
          onMouseLeave={(e) => {
            if (screen !== tab)
              (e.currentTarget as HTMLButtonElement).style.opacity = "0.5";
          }}
        >
          <MaterialIcon name={icon} size={17} />
        </button>
      ))}
    </div>
  );
}
