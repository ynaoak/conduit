import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { MaterialIcon } from "./MaterialIcon";

export interface SelectOption {
  id: string;
  label: string;
  /** Material Symbols name shown at the head of the row */
  icon?: string;
}

interface FooterSelectProps {
  /** Icon on the button itself — usually the current option's */
  icon: string;
  /** Short text on the button, e.g. a language code. Omit for icon-only. */
  badge?: string;
  /** Describes the current state: "Theme: light". Used as the tooltip and
   *  the accessible name. */
  label: string;
  /** Appended to the tooltip to say the button opens something */
  hint: string;
  options: SelectOption[];
  value: string;
  onChange: (id: string) => void;
}

/**
 * A footer control that opens its options instead of cycling through them.
 *
 * A cycle button hides both what the choices are and how many presses it
 * takes to reach one, which gets worse with every language we add. Showing
 * the list with the current entry ticked answers both at a glance.
 */
export function FooterSelect({
  icon,
  badge,
  label,
  hint,
  options,
  value,
  onChange,
}: FooterSelectProps) {
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const [anchor, setAnchor] = useState({ left: 0, bottom: 0 });

  // The footer sits at the bottom of the window, so the menu goes above the
  // button rather than below it where there is no room.
  useLayoutEffect(() => {
    if (!open) return;
    const rect = buttonRef.current?.getBoundingClientRect();
    if (!rect) return;
    setAnchor({ left: rect.left, bottom: window.innerHeight - rect.top + 6 });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(false);
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // Escape would otherwise reach the launcher's own handler and hide
      // the whole window; while the menu is open it belongs to the menu.
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("wheel", close);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("wheel", close);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [open]);

  return (
    <>
      <button
        ref={buttonRef}
        style={{ ...buttonStyle, background: open ? "var(--bg-hover)" : "transparent" }}
        title={`${label}（${hint}）`}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={() => setOpen((v) => !v)}
      >
        <MaterialIcon name={icon} size={14} />
        {badge && <span style={badgeStyle}>{badge}</span>}
        <MaterialIcon name="expand_less" size={12} />
      </button>
      {open && (
        <div
          role="menu"
          style={{ ...menuStyle, left: anchor.left, bottom: anchor.bottom }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {options.map((option) => {
            const selected = option.id === value;
            return (
              <div
                key={option.id}
                role="menuitemradio"
                aria-checked={selected}
                style={{
                  ...itemStyle,
                  background: selected ? "var(--bg-selected)" : "transparent",
                  color: selected ? "var(--text-accent)" : "var(--text-primary)",
                }}
                onClick={() => {
                  setOpen(false);
                  if (!selected) onChange(option.id);
                }}
              >
                <span style={itemIconStyle}>
                  {option.icon && <MaterialIcon name={option.icon} size={14} />}
                </span>
                <span style={{ flex: 1 }}>{option.label}</span>
                {/* Reserve the tick's width on every row so the labels do
                    not shift as the selection moves */}
                <span style={itemIconStyle}>
                  {selected && <MaterialIcon name="check" size={14} />}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}

const buttonStyle: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: "2px",
  border: "none",
  borderRadius: "999px",
  color: "var(--text-secondary)",
  cursor: "pointer",
  padding: "3px 5px",
  // the footer is a drag region; the button opts back out of it
  pointerEvents: "auto",
};

const badgeStyle: CSSProperties = {
  fontSize: "10px",
  fontWeight: 600,
  letterSpacing: "0.3px",
};

const menuStyle: CSSProperties = {
  position: "fixed",
  minWidth: "168px",
  background: "var(--bg-secondary)",
  border: "1px solid var(--border-color)",
  // M3 menus use the extra-small (4px) shape
  borderRadius: "4px",
  boxShadow: "var(--shadow-menu)",
  padding: "4px",
  zIndex: 20,
  pointerEvents: "auto",
  animation: "windowAppear 90ms ease-out",
};

const itemStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "8px",
  padding: "6px 8px",
  borderRadius: "4px",
  fontSize: "12px",
  cursor: "pointer",
  whiteSpace: "nowrap",
};

const itemIconStyle: CSSProperties = {
  display: "inline-flex",
  width: "14px",
  flexShrink: 0,
};
