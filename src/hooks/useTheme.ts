import { useEffect } from "react";
import type { AppConfig } from "../lib/types";

export function useTheme(config: AppConfig | null) {
  useEffect(() => {
    if (!config) return;
    const t = config.theme;
    const root = document.documentElement;
    root.style.setProperty("--bg-primary", t.bg_primary);
    root.style.setProperty("--bg-secondary", t.bg_secondary);
    root.style.setProperty("--bg-hover", t.bg_hover);
    root.style.setProperty("--bg-selected", t.bg_selected);
    root.style.setProperty("--text-primary", t.text_primary);
    root.style.setProperty("--text-secondary", t.text_secondary);
    root.style.setProperty("--text-accent", t.text_accent);
    root.style.setProperty("--border-color", t.border_color);
    root.style.setProperty("--border-radius", t.border_radius);
  }, [config]);
}
