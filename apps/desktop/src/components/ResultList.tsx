import { useEffect, useRef, type CSSProperties, type ReactNode } from "react";
import type { SearchResult } from "../lib/types";
import { ResultItem } from "./ResultItem";
import { AppIcon } from "./AppIcon";
import { MaterialIcon } from "./MaterialIcon";
import { useStrings } from "../lib/i18n";
import type { GridRange } from "../lib/dashboardGrid";

interface ResultListProps {
  results: SearchResult[];
  selectedIndex: number;
  onSelect: (index: number) => void;
  onExecute: (result: SearchResult) => void;
  isLoading: boolean;
  /** Dashboard mode (empty query): insert section headers between plugins */
  showSections: boolean;
  /** Empty-query dashboard with nothing pinned: one-line how-to-pin tip */
  pinHint?: string | null;
  onContextMenu?: (result: SearchResult, x: number, y: number) => void;
  /** Range drawn as a multi-column grid (dashboard frequent apps) */
  grid?: GridRange | null;
}

/** Section labels for the empty-query dashboard (keys in `manage`) */
const SECTION_LABELS: Record<string, string> = {
  "conduit.app-launcher": "group_frequent",
  "conduit.window-switcher": "group_windows",
  "conduit.clipboard-history": "group_clipboard",
};

const sectionHeaderStyle: CSSProperties = {
  fontSize: "10px",
  fontWeight: 600,
  letterSpacing: "0.5px",
  textTransform: "uppercase",
  color: "var(--text-secondary)",
  padding: "8px 28px 4px",
};

const pinHintStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "5px",
  fontSize: "11px",
  color: "var(--text-secondary)",
  padding: "4px 28px 2px",
};

function gridStyle(columns: number): CSSProperties {
  return {
    display: "grid",
    // minmax(0, 1fr): a long app name must shrink instead of widening its column
    gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
    rowGap: "2px",
  };
}

const containerStyle: CSSProperties = {
  flex: 1,
  overflowY: "auto",
  padding: "4px 0",
};

const emptyStyle: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  height: "100%",
  color: "var(--text-secondary)",
  fontSize: "14px",
  gap: "8px",
  padding: "40px 20px",
};

export function ResultList({
  results,
  selectedIndex,
  onSelect,
  onExecute,
  isLoading,
  showSections,
  pinHint,
  onContextMenu,
  grid = null,
}: ResultListProps) {
  const s = useStrings();
  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLDivElement | null)[]>([]);

  // Scroll selected item into view
  useEffect(() => {
    itemRefs.current[selectedIndex]?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const renderItem = (result: SearchResult, index: number) => (
    <div
      key={result.id}
      ref={(el) => {
        itemRefs.current[index] = el;
      }}
      style={{ minWidth: 0 }}
    >
      <ResultItem
        result={result}
        isSelected={index === selectedIndex}
        onClick={() => {
          onSelect(index);
          onExecute(result);
        }}
        onContextMenu={(r, x, y) => {
          onSelect(index);
          onContextMenu?.(r, x, y);
        }}
      />
    </div>
  );

  const renderHeader = (index: number) => {
    // Results are score-ordered so each plugin forms a contiguous block;
    // a header goes wherever the plugin changes (display only, indices
    // used by keyboard navigation are unaffected)
    const result = results[index];
    const label =
      showSections &&
      result.plugin_id !== results[index - 1]?.plugin_id &&
      SECTION_LABELS[result.plugin_id];
    return label ? (
      <div key={`header-${index}`} style={sectionHeaderStyle}>
        {s("manage", label)}
      </div>
    ) : null;
  };

  const renderRows = () => {
    const rows: ReactNode[] = [];
    let index = 0;
    while (index < results.length) {
      rows.push(renderHeader(index));
      if (grid && index === grid.start) {
        rows.push(
          <div key="dashboard-grid" style={gridStyle(grid.columns)}>
            {results.slice(grid.start, grid.end).map((r, i) => renderItem(r, grid.start + i))}
          </div>,
        );
        index = grid.end;
      } else {
        rows.push(renderItem(results[index], index));
        index++;
      }
    }
    return rows;
  };

  if (results.length === 0 && !isLoading) {
    return (
      <div style={emptyStyle}>
        <AppIcon size={48} style={{ opacity: 0.85 }} />
        <span>{s("app", "empty_title")}</span>
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "5px",
            fontSize: "12px",
            opacity: 0.7,
          }}
        >
          {/* the last emoji the Material Symbols pass missed */}
          <MaterialIcon name="deployed_code" size={14} />
          {s("app", "empty_hint")}
        </span>
        {pinHint && (
          <span style={{ ...pinHintStyle, padding: 0, opacity: 0.7 }}>
            <MaterialIcon name="push_pin" size={13} />
            {pinHint}
          </span>
        )}
      </div>
    );
  }

  return (
    <div style={containerStyle} ref={listRef}>
      {pinHint && (
        <div style={pinHintStyle}>
          <MaterialIcon name="push_pin" size={13} />
          {pinHint}
        </div>
      )}
      {renderRows()}
    </div>
  );
}
