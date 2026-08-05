import type { CSSProperties } from "react";

interface KeywordChipsProps {
  onPick: (prefix: string) => void;
}

/** Built-in keyword shortcuts, surfaced for discoverability */
const KEYWORDS: { prefix: string; label: string; hint: string }[] = [
  { prefix: "f ", label: "f", hint: "ファイル" },
  { prefix: "w ", label: "w", hint: "ウィンドウ" },
  { prefix: "cb ", label: "cb", hint: "クリップ" },
  { prefix: "ps ", label: "ps", hint: "ポート/プロセス" },
  { prefix: "=", label: "=", hint: "計算" },
];

const rowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "6px",
  padding: "8px 20px 4px",
  flexShrink: 0,
};

const chipStyle: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: "5px",
  background: "var(--bg-secondary)",
  border: "1px solid var(--border-color)",
  borderRadius: "999px",
  fontSize: "11px",
  color: "var(--text-secondary)",
  padding: "2px 10px",
  cursor: "pointer",
  transition: "border-color var(--transition-fast), color var(--transition-fast)",
};

const prefixStyle: CSSProperties = {
  color: "var(--text-accent)",
  fontFamily: "Consolas, monospace",
  fontWeight: 600,
};

export function KeywordChips({ onPick }: KeywordChipsProps) {
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
          {hint}
        </button>
      ))}
    </div>
  );
}
