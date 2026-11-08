import { useEffect, useState, type CSSProperties } from "react";
import { listPlugins } from "../lib/ipc";
import { useLanguage, useStrings } from "../lib/i18n";
import type { PluginInfo } from "../lib/types";

interface KeywordChipsProps {
  onPick: (prefix: string) => void;
}

/** Reading order, by plugin id. The registry answers in id order, which
 *  is stable but says nothing about what a new user needs first; files
 *  and windows do more work than Base64 does. Anything not named here
 *  (a plugin added later) still gets a chip, after these. */
const CHIP_ORDER = [
  "conduit.file-search",
  "conduit.window-switcher",
  "conduit.clipboard-history",
  "conduit.process-monitor",
  "conduit.base64",
  "conduit.json",
  "conduit.calculator",
];

/** The label under the keyword: short enough for a row of chips, which
 *  the plugin's own name is not ("クリップ" against "クリップボード履歴").
 *  Keyed by plugin id in the `keywords` section; without an entry the
 *  plugin name is used as-is. */
function hintKey(id: string): string {
  return id.replace(/^conduit\./, "").replace(/-/g, "_");
}

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

/** Built-in keyword shortcuts, surfaced for discoverability.
 *
 *  Read from the registry rather than listed here: a chip is a promise
 *  that typing it does something, and a plugin switched off in the
 *  settings tab (or a keyword renamed in its manifest) would otherwise
 *  keep its chip and do nothing. */
export function KeywordChips({ onPick }: KeywordChipsProps) {
  const s = useStrings();
  const lang = useLanguage();
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);

  useEffect(() => {
    listPlugins()
      .then(setPlugins)
      .catch(() => setPlugins([]));
  }, [lang]);

  const chips = plugins
    .filter((plugin) => plugin.enabled && plugin.keyword)
    .sort((a, b) => {
      const ia = CHIP_ORDER.indexOf(a.id);
      const ib = CHIP_ORDER.indexOf(b.id);
      return (ia < 0 ? CHIP_ORDER.length : ia) - (ib < 0 ? CHIP_ORDER.length : ib);
    });

  if (chips.length === 0) return null;

  return (
    <div style={rowStyle}>
      {chips.map((plugin) => {
        const keyword = plugin.keyword as string;
        // "=1+2" takes no space; "f foo" does. Symbolic keywords are the
        // ones that are not a word, which is also how the Rust side
        // decides whether a keyword needs a separator.
        const prefix = /[a-z0-9]$/i.test(keyword) ? `${keyword} ` : keyword;
        // No entry for this plugin: the catalog hands back the key itself
        const hint = hintKey(plugin.id);
        const label = s("keywords", hint);
        return (
          <button
            key={plugin.id}
            style={chipStyle}
            onClick={() => onPick(prefix)}
            title={plugin.description}
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
            <span style={prefixStyle}>{keyword}</span>
            {label === hint ? plugin.name : label}
          </button>
        );
      })}
    </div>
  );
}
