import type { CSSProperties, ReactNode } from "react";
import type { SearchResult } from "../lib/types";
import { MaterialIcon, hasMaterialIcon } from "./MaterialIcon";

interface ResultItemProps {
  result: SearchResult;
  isSelected: boolean;
  onClick: () => void;
  /** Right-click opens the custom action menu at the pointer */
  onContextMenu?: (result: SearchResult, x: number, y: number) => void;
  /** Dense single-line variant for the multi-column launcher grid: the
   *  subtitle moves to the tooltip and the shortcut badge is dropped */
  compact?: boolean;
}

function getIconDisplay(icon: SearchResult["icon"], size: number): ReactNode {
  switch (icon.type) {
    case "Emoji":
      return icon.value;
    case "Named":
      // Material symbol names sent by the built-in plugins
      if (hasMaterialIcon(icon.value))
        return (
          <MaterialIcon
            name={icon.value}
            size={Math.round(size * 0.62)}
            color="var(--text-secondary)"
          />
        );
      return icon.value.charAt(0).toUpperCase();
    case "Base64":
      // Real application icon extracted on the Rust side
      return (
        <img
          src={`data:image/png;base64,${icon.value}`}
          alt=""
          style={{ width: `${size}px`, height: `${size}px`, objectFit: "contain" }}
        />
      );
    default:
      return "?";
  }
}

/** Title with matched chars highlighted (indices are char positions) */
function HighlightedTitle({
  title,
  indices,
}: {
  title: string;
  indices?: number[];
}) {
  if (!indices || indices.length === 0) return <>{title}</>;
  const matched = new Set(indices);
  return (
    <>
      {Array.from(title).map((char, i) =>
        matched.has(i) ? (
          <span
            key={i}
            style={{ color: "var(--text-accent)", fontWeight: 600 }}
          >
            {char}
          </span>
        ) : (
          char
        ),
      )}
    </>
  );
}

export function ResultItem({
  result,
  isSelected,
  onClick,
  onContextMenu,
  compact = false,
}: ResultItemProps) {
  const containerStyle: CSSProperties = {
    display: "flex",
    alignItems: "center",
    padding: compact ? "0 8px" : "0 20px",
    height: compact ? "var(--result-height-compact)" : "var(--result-height)",
    cursor: "pointer",
    background: isSelected ? "var(--bg-selected)" : "transparent",
    transition: "background var(--transition-fast)",
    borderRadius: compact ? "8px" : "12px",
    margin: compact ? 0 : "0 8px",
  };

  const iconSize = compact ? 22 : 32;
  const iconContainerStyle: CSSProperties = {
    width: `${iconSize}px`,
    height: `${iconSize}px`,
    borderRadius: compact ? "5px" : "8px",
    background: "var(--bg-secondary)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    fontSize: compact ? "12px" : "16px",
    marginRight: compact ? "8px" : "12px",
    flexShrink: 0,
  };

  const textContainerStyle: CSSProperties = {
    flex: 1,
    overflow: "hidden",
    minWidth: 0,
  };

  const titleStyle: CSSProperties = {
    fontSize: compact ? "12px" : "14px",
    color: "var(--text-primary)",
    whiteSpace: "nowrap",
    overflow: "hidden",
    textOverflow: "ellipsis",
  };

  const subtitleStyle: CSSProperties = {
    fontSize: "12px",
    color: "var(--text-secondary)",
    whiteSpace: "nowrap",
    overflow: "hidden",
    textOverflow: "ellipsis",
    marginTop: "1px",
  };

  const hintStyle: CSSProperties = {
    fontSize: "11px",
    color: "var(--text-secondary)",
    background: "var(--bg-secondary)",
    padding: "2px 8px",
    borderRadius: "4px",
    marginLeft: "8px",
    flexShrink: 0,
    opacity: isSelected ? 1 : 0,
    transition: "opacity var(--transition-fast)",
  };

  return (
    <div
      style={containerStyle}
      // Compact rows drop the subtitle line, so the full text (for apps: the
      // executable path and launch count) is only reachable on hover
      title={compact ? [result.title, result.subtitle].filter(Boolean).join("\n") : undefined}
      onClick={onClick}
      onContextMenu={(e) => {
        if (!onContextMenu) return;
        e.preventDefault();
        e.stopPropagation();
        onContextMenu(result, e.clientX, e.clientY);
      }}
      onMouseEnter={(e) => {
        if (!isSelected)
          (e.currentTarget as HTMLDivElement).style.background =
            "var(--bg-hover)";
      }}
      onMouseLeave={(e) => {
        if (!isSelected)
          (e.currentTarget as HTMLDivElement).style.background = "transparent";
      }}
    >
      <div style={iconContainerStyle}>
        {getIconDisplay(result.icon, compact ? 16 : 20)}
      </div>
      <div style={textContainerStyle}>
        <div style={titleStyle}>
          <HighlightedTitle title={result.title} indices={result.match_indices} />
        </div>
        {!compact && result.subtitle && (
          <div style={subtitleStyle}>{result.subtitle}</div>
        )}
      </div>
      {!compact && isSelected && result.actions[0] && (
        <div style={hintStyle}>{result.actions[0].shortcut ?? "Enter"}</div>
      )}
    </div>
  );
}
