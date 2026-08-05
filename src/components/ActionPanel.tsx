import type { CSSProperties } from "react";
import type { Action } from "../lib/types";

interface ActionPanelProps {
  actions: Action[];
  selectedIndex: number;
  onHover: (index: number) => void;
  onSelect: (actionId: string) => void;
}

const panelStyle: CSSProperties = {
  position: "absolute",
  bottom: "40px",
  right: "12px",
  minWidth: "220px",
  background: "var(--bg-secondary)",
  border: "1px solid var(--border-color)",
  borderRadius: "8px",
  boxShadow: "0 8px 24px rgba(0, 0, 0, 0.4)",
  padding: "4px",
  zIndex: 12,
  animation: "windowAppear 100ms ease-out",
};

const headerStyle: CSSProperties = {
  fontSize: "10px",
  color: "var(--text-secondary)",
  padding: "4px 10px 2px",
  textTransform: "uppercase",
  letterSpacing: "0.5px",
};

export function ActionPanel({
  actions,
  selectedIndex,
  onHover,
  onSelect,
}: ActionPanelProps) {
  return (
    <div style={panelStyle}>
      <div style={headerStyle}>アクション</div>
      {actions.map((action, index) => {
        const itemStyle: CSSProperties = {
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "16px",
          padding: "6px 10px",
          borderRadius: "6px",
          fontSize: "13px",
          cursor: "pointer",
          color: "var(--text-primary)",
          background:
            index === selectedIndex ? "var(--bg-selected)" : "transparent",
        };
        return (
          <div
            key={action.id}
            style={itemStyle}
            onMouseEnter={() => onHover(index)}
            onClick={() => onSelect(action.id)}
          >
            <span>{action.title}</span>
            {action.shortcut && (
              <span
                style={{
                  fontSize: "10px",
                  color: "var(--text-secondary)",
                  background: "var(--bg-primary)",
                  padding: "1px 6px",
                  borderRadius: "4px",
                }}
              >
                {action.shortcut}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}
