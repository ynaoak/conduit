import type { CSSProperties } from "react";
import { useStrings } from "../lib/i18n";

interface KeywordChipsProps {
  onPick: (prefix: string) => void;
}

/** Built-in keyword shortcuts, surfaced for discoverability */
/** hint is a key in the `keywords` locale section */
const KEYWORDS: { prefix: string; label: string; hint: string }[] = [
  { prefix: "f ", label: "f", hint: "files" },
  { prefix: "w ", label: "w", hint: "windows" },
  { prefix: "cb ", label: "cb", hint: "clipboard" },
  { prefix: "ps ", label: "ps", hint: "processes" },
  { prefix: "b64 ", label: "b64", hint: "base64" },
  { prefix: "json ", label: "json", hint: "json" },
  { prefix: "=", label: "=", hint: "calc" },
];

const rowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  flexWrap: "wrap",
  gap: "6px",
  padding: "8px 20px 4px",
  flexShrink: 0,
};

const chipStyle: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: "5px",
  background: "transparent",
  border: "1px solid var(--border-color)",
  // M3 chips are 8px-rounded rectangles, not pills
  borderRadius: "8px",
  fontSize: "11px",
  color: "var(--text-secondary)",
  padding: "3px 12px",
  cursor: "pointer",
  transition: "border-color var(--transition-fast), color var(--transition-fast)",
};

const prefixStyle: CSSProperties = {
  color: "var(--text-accent)",
  fontFamily: "Consolas, monospace",
  fontWeight: 600,
};

export function KeywordChips({ onPick }: KeywordChipsProps) {
  const s = useStrings();
  return (
    <div style={rowStyle}>
      {KEYWORDS.map(({ prefix, label, hint }) => (
        <button
          key={prefix}
          style={chipStyle}
          onClick={() => onPick(prefix)}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLButtonElement).style.borderColor =
              "var(--text-accent)";
            (e.currentTarget as HTMLButtonElement).style.color =
              "var(--text-primary)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLButtonElement).style.borderColor =
              "var(--border-color)";
            (e.currentTarget as HTMLButtonElement).style.color =
              "var(--text-secondary)";
          }}
        >
          <span style={prefixStyle}>{label}</span>
          {s("keywords", hint)}
        </button>
      ))}
    </div>
  );
}
