import { useCallback, useEffect, useState, type CSSProperties } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { listPlugins, saveConfig } from "../lib/ipc";
import { useLanguage, useStrings } from "../lib/i18n";
import type { AppConfig, PluginInfo } from "../lib/types";
import { MaterialIcon, hasMaterialIcon } from "./MaterialIcon";

interface SettingsTabProps {
  config: AppConfig | null;
  onConfigSaved: (config: AppConfig) => void;
  onNotify: (message: string) => void;
}

/** The engines `config::resolve_search_url` knows by name. Anything else
 *  in the setting is a custom URL template, which is how the fourth
 *  choice is stored — there is no "custom" value, only a URL. */
const ENGINES = ["google", "duckduckgo", "bing"] as const;
const ENGINE_LABELS: Record<string, string> = {
  google: "Google",
  duckduckgo: "DuckDuckGo",
  bing: "Bing",
};

/** Fewer than this and the list cannot show one of each kind; more and
 *  the window is scrolling past what a keyboard user will ever reach. */
const MIN_RESULTS = 5;
const MAX_RESULTS = 50;

/** Everything is a Windows program, so the row would be dead weight
 *  anywhere else. WebView2's UA carries "Windows NT" — the same string
 *  main.tsx reads to decide which corner the window controls go in. */
const IS_WINDOWS = /Windows/.test(navigator.userAgent);

export function SettingsTab({ config, onConfigSaved, onNotify }: SettingsTabProps) {
  const s = useStrings();
  const lang = useLanguage();
  const [plugins, setPlugins] = useState<PluginInfo[] | null>(null);

  // Names and descriptions are localized in Rust and arrive as finished
  // strings, so a language switch has to re-ask rather than re-render
  useEffect(() => {
    listPlugins()
      .then(setPlugins)
      .catch(() => setPlugins([]));
  }, [lang]);

  const persist = useCallback(
    (next: AppConfig) => {
      // Optimistic, like the About tab: the toggle moves now and a failed
      // write says so, rather than the UI waiting on the filesystem
      onConfigSaved(next);
      saveConfig(next).catch((err) =>
        onNotify(s("settings", "save_failed", { error: String(err) })),
      );
    },
    [onConfigSaved, onNotify, s],
  );

  const engine = config?.search.web_search_engine ?? "google";
  const named = (ENGINES as readonly string[]).includes(engine.toLowerCase());
  // Held locally while being typed: every keystroke in here would
  // otherwise rewrite config.json, and a half-typed URL is not a setting
  const [customUrl, setCustomUrl] = useState(named ? "" : engine);
  const [maxResults, setMaxResults] = useState(String(config?.search.max_results ?? 20));
  const [everything, setEverything] = useState(config?.search.everything_path ?? "");

  // config arrives asynchronously and a save round-trips through it, so
  // the drafts adopt whatever is stored whenever that changes. Typing does
  // not change what is stored, so this never fights the keyboard.
  const storedMax = config?.search.max_results;
  const storedEverything = config?.search.everything_path;
  useEffect(() => {
    if (storedMax === undefined) return;
    setMaxResults(String(storedMax));
    setEverything(storedEverything ?? "");
    setCustomUrl(named ? "" : engine);
  }, [storedMax, storedEverything, engine, named]);

  if (!config) {
    return <div style={emptyStyle}>{s("settings", "config_unavailable")}</div>;
  }

  const setSearch = (patch: Partial<AppConfig["search"]>) =>
    persist({ ...config, search: { ...config.search, ...patch } });

  const commitMaxResults = () => {
    const parsed = Number.parseInt(maxResults, 10);
    const clamped = Number.isNaN(parsed)
      ? config.search.max_results
      : Math.min(MAX_RESULTS, Math.max(MIN_RESULTS, parsed));
    setMaxResults(String(clamped));
    if (clamped !== config.search.max_results) setSearch({ max_results: clamped });
  };

  const pickEngine = (value: string) => {
    if (value === "custom") {
      // Nothing is saved until a URL is actually typed: writing "custom"
      // into the setting would resolve to Google and look like a bug
      setCustomUrl(customUrl || "");
      setSearch({ web_search_engine: customUrl.includes("{query}") ? customUrl : "" });
      return;
    }
    setSearch({ web_search_engine: value });
  };

  const commitCustomUrl = () => {
    const url = customUrl.trim();
    if (!url.includes("{query}")) return; // the hint below says why
    setSearch({ web_search_engine: url });
  };

  const browseForEverything = () => {
    openFileDialog({
      multiple: false,
      filters: [{ name: "es.exe", extensions: ["exe"] }],
    })
      .then((path) => {
        if (typeof path !== "string") return; // cancelled
        setEverything(path);
        setSearch({ everything_path: path });
      })
      .catch((err) => onNotify(s("settings", "save_failed", { error: String(err) })));
  };

  const togglePlugin = (plugin: PluginInfo) => {
    const enabled = { ...config.plugins.enabled, [plugin.id]: !plugin.enabled };
    persist({ ...config, plugins: { enabled } });
    setPlugins(
      (list) =>
        list?.map((p) => (p.id === plugin.id ? { ...p, enabled: !p.enabled } : p)) ?? null,
    );
  };

  // The select shows "custom" for anything that is not a known engine —
  // including the empty string a just-picked custom entry starts as
  const engineValue = named ? engine.toLowerCase() : "custom";

  return (
    <div>
      <section style={cardStyle}>
        <div style={headingStyle}>{s("settings", "search_heading")}</div>

        <div style={rowStyle}>
          <span style={labelStyle}>{s("settings", "max_results")}</span>
          <input
            type="number"
            min={MIN_RESULTS}
            max={MAX_RESULTS}
            value={maxResults}
            onChange={(e) => setMaxResults(e.target.value)}
            onBlur={commitMaxResults}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitMaxResults();
            }}
            style={{ ...inputStyle, width: "72px" }}
          />
          <span style={hintStyle}>
            {s("settings", "max_results_hint", { min: MIN_RESULTS, max: MAX_RESULTS })}
          </span>
        </div>

        <div style={rowStyle}>
          <span style={labelStyle}>{s("settings", "engine")}</span>
          <select
            value={engineValue}
            onChange={(e) => pickEngine(e.target.value)}
            style={inputStyle}
          >
            {ENGINES.map((id) => (
              <option key={id} value={id}>
                {ENGINE_LABELS[id]}
              </option>
            ))}
            <option value="custom">{s("settings", "engine_custom")}</option>
          </select>
        </div>

        {engineValue === "custom" && (
          <div style={rowStyle}>
            <span style={labelStyle} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <input
                type="text"
                value={customUrl}
                placeholder={s("settings", "engine_custom_placeholder")}
                onChange={(e) => setCustomUrl(e.target.value)}
                onBlur={commitCustomUrl}
                onKeyDown={(e) => {
                  if (e.key === "Enter") commitCustomUrl();
                }}
                style={{ ...inputStyle, width: "100%" }}
              />
              <div
                style={{
                  ...hintStyle,
                  display: "block",
                  marginTop: "4px",
                  ...(customUrl.trim() && !customUrl.includes("{query}")
                    ? { color: "var(--text-accent)" }
                    : {}),
                }}
              >
                {customUrl.trim() && !customUrl.includes("{query}")
                  ? s("settings", "engine_custom_invalid")
                  : s("settings", "engine_custom_hint")}
              </div>
            </div>
          </div>
        )}

        {IS_WINDOWS && (
          <div style={rowStyle}>
            <span style={labelStyle}>{s("settings", "everything")}</span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: "flex", gap: "6px" }}>
                <input
                  type="text"
                  value={everything}
                  placeholder={s("settings", "everything_placeholder")}
                  onChange={(e) => setEverything(e.target.value)}
                  onBlur={() => setSearch({ everything_path: everything.trim() })}
                  style={{ ...inputStyle, flex: 1, minWidth: 0 }}
                />
                <button style={buttonStyle} onClick={browseForEverything}>
                  {s("settings", "browse")}
                </button>
                {everything !== "" && (
                  <button
                    style={buttonStyle}
                    onClick={() => {
                      setEverything("");
                      setSearch({ everything_path: "" });
                    }}
                  >
                    {s("settings", "clear")}
                  </button>
                )}
              </div>
              <div style={{ ...hintStyle, display: "block", marginTop: "4px" }}>
                {s("settings", "everything_hint")}
              </div>
            </div>
          </div>
        )}
      </section>

      <section style={cardStyle}>
        <div style={headingStyle}>{s("settings", "window_heading")}</div>
        <label style={toggleRowStyle}>
          <input
            type="checkbox"
            checked={config.window.hide_on_blur}
            onChange={() =>
              persist({
                ...config,
                window: { ...config.window, hide_on_blur: !config.window.hide_on_blur },
              })
            }
          />
          <span>
            <div>{s("settings", "hide_on_blur")}</div>
            <div style={{ ...hintStyle, display: "block" }}>
              {s("settings", "hide_on_blur_hint")}
            </div>
          </span>
        </label>
      </section>

      <section style={cardStyle}>
        <div style={headingStyle}>{s("settings", "plugins_heading")}</div>
        <div style={{ ...hintStyle, display: "block", marginBottom: "8px" }}>
          {s("settings", "plugins_hint")}
        </div>
        {plugins?.length === 0 && (
          <div style={hintStyle}>{s("settings", "plugins_empty")}</div>
        )}
        {plugins?.map((plugin) => (
          <label
            key={plugin.id}
            style={{
              ...pluginRowStyle,
              ...(plugin.enabled ? {} : { opacity: 0.55 }),
            }}
          >
            <MaterialIcon
              name={hasMaterialIcon(plugin.icon) ? plugin.icon : "extension"}
              size={18}
              color="var(--text-secondary)"
            />
            <span style={{ flex: 1, minWidth: 0 }}>
              <span style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                <span style={{ fontSize: "13px", color: "var(--text-primary)" }}>
                  {plugin.name}
                </span>
                {plugin.keyword && <span style={keywordStyle}>{plugin.keyword}</span>}
                {plugin.keyword_only && (
                  <span style={hintStyle}>{s("settings", "plugins_keyword_only")}</span>
                )}
              </span>
              <span style={{ ...hintStyle, display: "block" }}>{plugin.description}</span>
            </span>
            <input
              type="checkbox"
              checked={plugin.enabled}
              onChange={() => togglePlugin(plugin)}
            />
          </label>
        ))}
      </section>
    </div>
  );
}

const cardStyle: CSSProperties = {
  background: "var(--bg-secondary)",
  border: "1px solid var(--border-color)",
  borderRadius: "12px",
  padding: "12px 14px",
  marginBottom: "8px",
};

const headingStyle: CSSProperties = {
  fontSize: "13px",
  fontWeight: 600,
  color: "var(--text-primary)",
  marginBottom: "8px",
};

const rowStyle: CSSProperties = {
  display: "flex",
  alignItems: "flex-start",
  gap: "8px",
  padding: "6px 0",
};

const labelStyle: CSSProperties = {
  width: "140px",
  flexShrink: 0,
  fontSize: "12px",
  color: "var(--text-primary)",
  paddingTop: "4px",
};

const inputStyle: CSSProperties = {
  background: "var(--bg-primary)",
  color: "var(--text-primary)",
  border: "1px solid var(--border-color)",
  borderRadius: "6px",
  padding: "3px 8px",
  fontSize: "12px",
};

const buttonStyle: CSSProperties = {
  background: "transparent",
  border: "1px solid var(--border-color)",
  borderRadius: "999px",
  color: "var(--text-secondary)",
  fontSize: "11px",
  padding: "2px 10px",
  cursor: "pointer",
  flexShrink: 0,
};

/** The keyword as it is typed, in the same accent-on-mono the chips use */
const keywordStyle: CSSProperties = {
  fontFamily: "Consolas, monospace",
  fontWeight: 600,
  fontSize: "11px",
  color: "var(--text-accent)",
  border: "1px solid var(--border-color)",
  borderRadius: "6px",
  padding: "0 6px",
};

const hintStyle: CSSProperties = {
  fontSize: "11px",
  color: "var(--text-secondary)",
};

const toggleRowStyle: CSSProperties = {
  display: "flex",
  alignItems: "flex-start",
  gap: "8px",
  fontSize: "12px",
  color: "var(--text-primary)",
  cursor: "pointer",
};

const pluginRowStyle: CSSProperties = {
  display: "flex",
  alignItems: "flex-start",
  gap: "8px",
  padding: "7px 0",
  borderTop: "1px solid var(--border-color)",
  cursor: "pointer",
};

const emptyStyle: CSSProperties = {
  padding: "24px",
  fontSize: "13px",
  color: "var(--text-secondary)",
};
