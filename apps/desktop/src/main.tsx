import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { Loupe } from "./components/Loupe";
import { CaptureOverlay } from "./components/CaptureOverlay";
import { Recorder } from "./components/Recorder";

// Set before the first paint: window controls sit on opposite sides on
// macOS and Windows, and resolving this over IPC would render them in the
// wrong corner first and visibly jump. The UA string is the only thing
// available synchronously, and "Mac OS X" is stable in WKWebView's.
document.documentElement.dataset.platform =
  /Mac OS X|Macintosh/.test(navigator.userAgent) ? "macos" : "other";

// The launcher is one of several windows on the same bundle, told apart
// by the hash: the color loupe, the capture selection surface and the
// recorder controls each render one component and share nothing with the
// launcher but the build.
const WINDOWS: Record<string, () => React.ReactElement> = {
  "#loupe": Loupe,
  "#capture": CaptureOverlay,
  "#recorder": Recorder,
};

// Typed as possibly absent: the launcher's own hash is not in the map,
// and an index signature would otherwise promise a component for it.
const Standalone: (() => React.ReactElement) | undefined = WINDOWS[location.hash];
if (Standalone !== undefined) {
  document.documentElement.dataset.window = location.hash.slice(1);
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{Standalone !== undefined ? <Standalone /> : <App />}</React.StrictMode>,
);
