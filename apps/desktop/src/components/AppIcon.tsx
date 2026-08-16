import type { CSSProperties } from "react";
// The bundler emits this next to the other assets, so it loads from the
// app's own origin and stays inside the `img-src 'self'` CSP. Sourced from
// the same file the installer uses, rather than a second copy that could
// drift from it.
import iconUrl from "../../src-tauri/icons/128x128.png";

interface AppIconProps {
  size: number;
  style?: CSSProperties;
}

/** The Conduit mark. Decorative wherever it appears — the app name is
 *  always next to it — so it carries an empty alt. */
export function AppIcon({ size, style }: AppIconProps) {
  return (
    <img
      src={iconUrl}
      alt=""
      width={size}
      height={size}
      draggable={false}
      style={{ display: "block", flexShrink: 0, ...style }}
    />
  );
}
