import { useEffect } from "react";
import { setResolvedTheme } from "../lib/ipc";
import type { AppConfig } from "../lib/types";

const SLOTS: [string, keyof AppConfig["theme"]][] = [
  ["--bg-primary", "bg_primary"],
  ["--bg-secondary", "bg_secondary"],
  ["--bg-hover", "bg_hover"],
  ["--bg-selected", "bg_selected"],
  ["--text-primary", "text_primary"],
  ["--text-secondary", "text_secondary"],
  ["--text-accent", "text_accent"],
  ["--border-color", "border_color"],
  ["--border-radius", "border_radius"],
];

/** Launcher variable -> the role name tool windows see as
 *  `--conduit-<role>`. Reporting the resolved values (rather than letting
 *  each tool app carry its own palette) is what keeps a tool window the
 *  same color as the launcher, config overrides included. */
const REPORTED: [string, string][] = [
  ["surface", "--bg-primary"],
  ["surface-container", "--bg-secondary"],
  ["surface-high", "--bg-hover"],
  ["selected", "--bg-selected"],
  ["on-surface", "--text-primary"],
  ["on-variant", "--text-secondary"],
  ["primary", "--text-accent"],
  ["outline", "--border-color"],
  ["radius", "--border-radius"],
];

/** Perceived lightness of a CSS color string, 0 (black) to 1 (white).
 *  Parsed via the canvas 2D context so any CSS syntax works, not just hex. */
function lightness(color: string): number | null {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 1;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  ctx.fillStyle = "#000";
  ctx.fillStyle = color;
  // An unparsable color leaves fillStyle at the previous value
  if (ctx.fillStyle === "#000000" && color.trim() !== "#000000") {
    if (!/^(#000|black|rgb\(0, ?0, ?0\))$/i.test(color.trim())) return null;
  }
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
  // ITU-R BT.601 luma: close enough to judge dark vs light surfaces
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255;
}

/** Apply theme.mode (system / dark / light) as data-theme on the root
 *  element — the built-in M3 palettes live in global.css — plus any
 *  per-slot color overrides from config.json as inline styles.
 *
 *  The rendered background is then measured and reported to the backend:
 *  a config that overrides the colors (every config written before
 *  theme.mode existed does) can render dark while mode says "system", so
 *  mode alone would tell tool windows the wrong thing. */
export function useTheme(config: AppConfig | null) {
  useEffect(() => {
    if (!config) return;
    const t = config.theme;
    const root = document.documentElement;

    const report = () => {
      const styles = getComputedStyle(root);
      const colors: Record<string, string> = {};
      for (const [role, cssVar] of REPORTED) {
        const value = styles.getPropertyValue(cssVar).trim();
        if (value) colors[role] = value;
      }
      const l = lightness(colors.surface ?? "");
      if (l === null) return;
      setResolvedTheme({ mode: l > 0.5 ? "light" : "dark", colors }).catch(
        console.error,
      );
    };

    const media = window.matchMedia("(prefers-color-scheme: light)");
    const applyMode = () => {
      const light =
        t.mode === "light" || (t.mode !== "dark" && media.matches);
      root.dataset.theme = light ? "light" : "dark";
      report();
    };

    for (const [cssVar, key] of SLOTS) {
      const value = t[key];
      if (typeof value === "string" && value) {
        root.style.setProperty(cssVar, value);
      } else {
        root.style.removeProperty(cssVar);
      }
    }
    // Overrides first, then the mode — report() must see the final colors
    applyMode();

    // system mode follows OS theme changes live
    media.addEventListener("change", applyMode);
    return () => media.removeEventListener("change", applyMode);
  }, [config]);
}
