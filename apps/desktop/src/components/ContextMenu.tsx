import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import type { Action } from "../lib/types";

interface ContextMenuProps {
  x: number;
  y: number;
  actions: Action[];
  onSelect: (actionId: string) => void;
  onClose: () => void;
}

const MENU_WIDTH = 220;

const itemStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: "16px",
  padding: "6px 10px",
  borderRadius: "4px",
  fontSize: "13px",
  cursor: "pointer",
  color: "var(--text-primary)",
  whiteSpace: "nowrap",
};

const shortcutStyle: CSSProperties = {
  fontSize: "10px",
  color: "var(--text-secondary)",
  background: "var(--bg-primary)",
  padding: "1px 6px",
  borderRadius: "4px",
};

/** Right-click menu for a result row — replaces the WebView default menu */
export function ContextMenu({ x, y, actions, onSelect, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });
  const [hovered, setHovered] = useState(0);

  // Flip the menu when it would overflow the window
  useLayoutEffect(() => {
    const el = menuRef.current;
    if (!el) return;
    const { offsetWidth: width, offsetHeight: height } = el;
    setPosition({
      left: x + width > window.innerWidth ? Math.max(4, x - width) : x,
      top: y + height > window.innerHeight ? Math.max(4, y - height) : y,
    });
  }, [x, y]);

  // Any outside interaction dismisses the menu
  useEffect(() => {
    const close = () => onClose();
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("wheel", close);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("wheel", close);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [onClose]);

  return (
    <div
      ref={menuRef}
      style={{
        position: "fixed",
        left: position.left,
        top: position.top,
        minWidth: `${MENU_WIDTH}px`,
        background: "var(--bg-secondary)",
        border: "1px solid var(--border-color)",
        // M3 menus use the extra-small (4px) shape
        borderRadius: "4px",
        boxShadow: "var(--shadow-menu)",
        padding: "4px",
        zIndex: 20,
        animation: "windowAppear 90ms ease-out",
      }}
      // Keep our own mousedown from bubbling to the dismiss handler
      onMouseDown={(e) => e.stopPropagation()}
    >
      {actions.map((action, index) => (
        <div
          key={action.id}
          style={{
            ...itemStyle,
            background: index === hovered ? "var(--bg-selected)" : "transparent",
          }}
          onMouseEnter={() => setHovered(index)}
          onClick={() => {
            onSelect(action.id);
            onClose();
          }}
        >
          <span>{action.title}</span>
          {action.shortcut && <span style={shortcutStyle}>{action.shortcut}</span>}
        </div>
      ))}
    </div>
  );
}
