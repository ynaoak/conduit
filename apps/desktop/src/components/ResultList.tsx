import { useEffect, useRef, type CSSProperties } from "react";
import type { SearchResult } from "../lib/types";
import { ResultItem } from "./ResultItem";
import { AppIcon } from "./AppIcon";
import { MaterialIcon } from "./MaterialIcon";
import { useStrings } from "../lib/i18n";

interface ResultListProps {
  results: SearchResult[];
  selectedIndex: number;
  onSelect: (index: number) => void;
  onExecute: (result: SearchResult) => void;
  isLoading: boolean;
  /** Dashboard mode (empty query): insert section headers between plugins */
  showSections: boolean;
  onContextMenu?: (result: SearchResult, x: number, y: number) => void;
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
  onContextMenu,
}: ResultListProps) {
  const s = useStrings();
  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLDivElement | null)[]>([]);

  // Scroll selected item into view
  useEffect(() => {
    itemRefs.current[selectedIndex]?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

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
      </div>
    );
  }

  return (
    <div style={containerStyle} ref={listRef}>
      {results.map((result, index) => {
        // Results are score-ordered so each plugin forms a contiguous block;
        // a header goes wherever the plugin changes (display only, indices
        // used by keyboard navigation are unaffected)
        const showHeader =
          showSections &&
          result.plugin_id !== results[index - 1]?.plugin_id &&
          SECTION_LABELS[result.plugin_id];
        return (
          <div
            key={result.id}
            ref={(el) => {
              itemRefs.current[index] = el;
            }}
          >
            {showHeader && (
              <div style={sectionHeaderStyle}>
                {s("manage", SECTION_LABELS[result.plugin_id])}
              </div>
            )}
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
      })}
    </div>
  );
}
