import { createContext, useContext } from "react";

/** One locale file: sections of key -> string, plus `$meta` */
type Catalog = Record<string, Record<string, string>>;

/**
 * Every `locales/*.json`, embedded at build time. Discovered rather than
 * listed, so adding a language is only ever "drop in locales/<code>.json"
 * — nothing here (or in the Rust side, which generates the same list in
 * build.rs) enumerates the languages.
 */
const FILES = import.meta.glob<Catalog>("../../locales/*.json", {
  eager: true,
  import: "default",
});

export const CATALOGS: Record<string, Catalog> = Object.fromEntries(
  Object.entries(FILES).map(([path, catalog]) => [
    path.replace(/.*\/(.+)\.json$/, "$1"),
    catalog,
  ]),
);

/** Used when the chosen language lacks a key, and when the OS language
 *  matches nothing we ship. Must exist in locales/. */
const FALLBACK = "en";

/** Shipped language codes, in a stable order */
export const LANGUAGES = Object.keys(CATALOGS).sort();

/** Display name for a language, read from its own file so the picker
 *  needs no table of its own */
export function languageName(code: string): string {
  return CATALOGS[code]?.$meta?.name ?? code;
}

/** "system" (or anything unknown) follows the browser/OS language */
export function resolveLanguage(setting: string | undefined): string {
  if (setting && CATALOGS[setting]) return setting;
  for (const tag of navigator.languages ?? [navigator.language]) {
    // "ja-JP" should find "ja"
    const base = tag.split("-")[0]?.toLowerCase();
    if (base && CATALOGS[base]) return base;
  }
  return FALLBACK;
}

/** Reader for one language: `s("app", "close")`, with `{placeholder}`
 *  substitution and a fallback chain of language -> English -> the key,
 *  so a gap shows something identifiable rather than an empty label. */
export type Strings = (
  section: string,
  key: string,
  args?: Record<string, string | number>,
) => string;

export function makeStrings(lang: string): Strings {
  return (section, key, args) => {
    const value =
      CATALOGS[lang]?.[section]?.[key] ??
      CATALOGS[FALLBACK]?.[section]?.[key] ??
      key;
    if (!args) return value;
    return Object.entries(args).reduce(
      // split/join rather than replaceAll: the tsconfig target predates it
      (text, [name, replacement]) =>
        text.split(`{${name}}`).join(String(replacement)),
      value,
    );
  };
}

/** Language + reader for the whole tree; App provides it from config */
export interface LanguageContextValue {
  /** The resolved code actually in use ("ja" / "en" / …) */
  lang: string;
  s: Strings;
}

export const LanguageContext = createContext<LanguageContextValue>({
  lang: FALLBACK,
  s: makeStrings(FALLBACK),
});

/** `const s = useStrings()` in any component */
export function useStrings(): Strings {
  return useContext(LanguageContext).s;
}

export function useLanguage(): string {
  return useContext(LanguageContext).lang;
}
