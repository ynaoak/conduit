import type { CSSProperties, ReactNode } from "react";
import { MaterialIcon } from "./MaterialIcon";
import { useStrings } from "../lib/i18n";

interface SearchBarProps {
  value: string;
  onChange: (value: string) => void;
  /** Trailing content (the persistent screen tabs) */
  trailing?: ReactNode;
}

const containerStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  padding: "0 20px",
  height: "var(--input-height)",
  borderBottom: "1px solid var(--border-color)",
  flexShrink: 0,
};

const iconStyle: CSSProperties = {
  display: "inline-flex",
  marginRight: "12px",
  color: "var(--text-secondary)",
  flexShrink: 0,
};

const inputStyle: CSSProperties = {
  flex: 1,
  background: "transparent",
  border: "none",
  outline: "none",
  fontSize: "18px",
  color: "var(--text-primary)",
  fontFamily: "var(--font-family)",
  caretColor: "var(--text-accent)",
};

export function SearchBar({ value, onChange, trailing }: SearchBarProps) {
  const s = useStrings();
  return (
    <div style={containerStyle}>
      <span style={iconStyle}>
        <MaterialIcon name="search" size={22} />
      </span>
      <input
        id="search-input"
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={s("app", "search_placeholder")}
        style={inputStyle}
        autoFocus
        autoComplete="off"
        spellCheck={false}
      />
      {trailing}
    </div>
  );
}
