import type { CSSProperties } from "react";
import { chordLabel } from "../lib/keys";
import type { Keybindings } from "../lib/types";
import { AppIcon } from "./AppIcon";
import { FooterSelect, type SelectOption } from "./FooterSelect";
import { LANGUAGES, languageName, useStrings } from "../lib/i18n";

/** theme.mode choices. "system" stays reachable from the UI — without it
 *  the only way back to following the OS would be editing config.json. */
export const THEME_MODES = ["system", "light", "dark"] as const;
export type ThemeMode = (typeof THEME_MODES)[number];

const THEME_ICONS: Record<ThemeMode, string> = {
  system: "brightness_auto",
  light: "light_mode",
  dark: "dark_mode",
};

/** language choices: "system" first, then every shipped locale — built
 *  from the catalog so a new locales/<code>.json shows up here on its own */
export const LANGUAGE_MODES = ["system", ...LANGUAGES] as const;
export type LanguageMode = (typeof LANGUAGE_MODES)[number];

interface StatusBarProps {
  pluginName?: string;
  keybindings: Keybindings;
  themeMode: ThemeMode;
  onSelectTheme: (mode: ThemeMode) => void;
  /** The `language` setting, which may be "system" */
  languageMode: string;
  onSelectLanguage: (language: string) => void;
}

const containerStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  padding: "0 20px",
  height: "32px",
  borderTop: "1px solid var(--border-color)",
  fontSize: "11px",
  color: "var(--text-secondary)",
  flexShrink: 0,
  // The whole footer doubles as a window drag area
  cursor: "grab",
};

/** Children must not swallow mousedown: the drag region check only matches
 *  the element that carries data-tauri-drag-region itself. */
const passthroughStyle: CSSProperties = {
  pointerEvents: "none",
};

export function StatusBar({
  pluginName,
  keybindings,
  themeMode,
  onSelectTheme,
  languageMode,
  onSelectLanguage,
}: StatusBarProps) {
  const s = useStrings();
  // Plugin ids are "conduit.<name>"; the catalog keys off the name so a
  // missing entry falls back to the raw id rather than to nothing
  const displayName = pluginName
    ? s("app", `plugin_${pluginName.replace(/^conduit\./, "")}`)
    : "Conduit";

  // Hints follow the configured keybindings (first chord of each action)
  const moveLabel = `${chordLabel(keybindings.move_up)}${chordLabel(keybindings.move_down)}`;

  const themeOptions: SelectOption[] = THEME_MODES.map((mode) => ({
    id: mode,
    label: s("app", `theme_option_${mode}`),
    icon: THEME_ICONS[mode],
  }));
  const languageOptions: SelectOption[] = [
    { id: "system", label: s("app", "language_option_system"), icon: "brightness_auto" },
    // Each language is named in its own tongue, so it is legible even when
    // the UI is currently in one the reader does not speak
    ...LANGUAGES.map((code) => ({ id: code, label: languageName(code) })),
  ];

  const languageLabel =
    languageMode === "system"
      ? s("app", "language_system")
      : `${s("app", "language_label")}: ${languageName(languageMode)}`;
  const menuHint = s("app", "menu_open_hint");

  return (
    <div style={containerStyle} data-tauri-drag-region>
      <span style={{ display: "flex", alignItems: "center", gap: "6px" }}>
        <FooterSelect
          icon={THEME_ICONS[themeMode]}
          label={s("app", `theme_${themeMode}`)}
          hint={menuHint}
          options={themeOptions}
          value={themeMode}
          onChange={(id) => onSelectTheme(id as ThemeMode)}
        />
        <FooterSelect
          icon="language"
          badge={languageMode === "system" ? "AUTO" : languageMode.toUpperCase()}
          label={languageLabel}
          hint={menuHint}
          options={languageOptions}
          value={languageMode}
          onChange={onSelectLanguage}
        />
        <span style={{ ...passthroughStyle, display: "flex", alignItems: "center", gap: "6px" }}>
          <AppIcon size={16} style={{ opacity: 0.9 }} />
          {displayName}
        </span>
      </span>
      <span style={passthroughStyle}>
        <kbd style={kbdStyle}>{moveLabel}</kbd> {s("app", "hint_move")}{" "}
        <kbd style={kbdStyle}>{chordLabel(keybindings.execute)}</kbd>{" "}
        {s("app", "hint_execute")}{" "}
        <kbd style={kbdStyle}>{chordLabel(keybindings.action_panel)}</kbd>{" "}
        {s("app", "hint_actions")}{" "}
        <kbd style={kbdStyle}>{chordLabel(keybindings.toggle_pin)}</kbd>{" "}
        {s("app", "hint_pin")}{" "}
        <kbd style={kbdStyle}>{chordLabel(keybindings.close)}</kbd>{" "}
        {s("app", "hint_close")}
      </span>
    </div>
  );
}

const kbdStyle: CSSProperties = {
  background: "var(--bg-secondary)",
  padding: "1px 5px",
  borderRadius: "3px",
  fontSize: "10px",
  marginRight: "2px",
};
