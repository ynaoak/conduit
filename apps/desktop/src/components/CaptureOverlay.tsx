import { useCallback, useEffect, useRef, useState } from "react";
import {
  captureFrame,
  captureSelected,
  captureSurfaceFocus,
  captureSurfaceReady,
  resolvedLanguage,
  type CaptureSelection,
  type CaptureSurface,
} from "../lib/ipc";
import { makeStrings, resolveLanguage } from "../lib/i18n";

interface Drag {
  x0: number;
  y0: number;
  x: number;
  y: number;
}

/**
 * The selection surface: a window covering one monitor, showing a still of
 * that monitor taken a moment ago (see src-tauri/src/capture/region.rs).
 *
 * One of these is rendered per display. They are separate windows that
 * know nothing about each other; the session behind them is what makes
 * the first drag on any of them end all of them.
 *
 * Everything here is drawing and arithmetic. The rectangle it reports is in
 * fractions of the image, never pixels — this window works in CSS pixels
 * and the frame is in physical ones, and the ratio between them is exactly
 * what goes wrong on a high-DPI display.
 */
export function CaptureOverlay() {
  const [surface, setSurface] = useState<CaptureSurface | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [lang, setLang] = useState(() => resolveLanguage(undefined));
  const root = useRef<HTMLDivElement>(null);
  const image = useRef<HTMLImageElement>(null);
  /** The session takes one answer; a second would resolve the next one */
  const answered = useRef(false);

  const s = makeStrings(lang);

  const finish = useCallback((selection: CaptureSelection | null) => {
    if (answered.current) return;
    answered.current = true;
    // The window is closed by Rust once the session ends; a failure here
    // means the session already ended, and there is nothing to recover.
    captureSelected(selection).catch(() => {});
  }, []);

  useEffect(() => {
    captureFrame()
      .then(setSurface)
      .catch(() => finish(null));
    resolvedLanguage()
      .then(setLang)
      .catch(() => {});
  }, [finish]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") finish(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [finish]);

  /** Where a pointer event is, as a fraction of the surface */
  const fraction = (e: React.PointerEvent) => {
    const box = root.current?.getBoundingClientRect();
    if (!box || box.width === 0 || box.height === 0) return { x: 0, y: 0 };
    return {
      x: (e.clientX - box.left) / box.width,
      y: (e.clientY - box.top) / box.height,
    };
  };

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const at = fraction(e);
    // Capture the pointer so a drag that leaves the window still ends here
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag({ x0: at.x, y0: at.y, x: at.x, y: at.y });
  };

  const onPointerMove = (e: React.PointerEvent) => {
    if (!drag) return;
    const at = fraction(e);
    setDrag({ ...drag, x: at.x, y: at.y });
  };

  const onPointerUp = (e: React.PointerEvent) => {
    if (!drag) return;
    const at = fraction(e);
    setDrag(null);
    finish({
      x: drag.x0,
      y: drag.y0,
      width: at.x - drag.x0,
      height: at.y - drag.y0,
    });
  };

  // A drag reported backwards is the same rectangle, but CSS wants the
  // top-left corner and a positive size.
  const box = drag && {
    left: `${Math.min(drag.x0, drag.x) * 100}%`,
    top: `${Math.min(drag.y0, drag.y) * 100}%`,
    width: `${Math.abs(drag.x - drag.x0) * 100}%`,
    height: `${Math.abs(drag.y - drag.y0) * 100}%`,
  };

  // The readout is in the pixels the file will have, which is the image's
  // own size — not the window's, which is smaller by the scale factor.
  const pixels = drag && {
    width: Math.round(Math.abs(drag.x - drag.x0) * (image.current?.naturalWidth ?? 0)),
    height: Math.round(Math.abs(drag.y - drag.y0) * (image.current?.naturalHeight ?? 0)),
  };

  return (
    <div
      ref={root}
      className="capture-surface"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onContextMenu={(e) => {
        e.preventDefault();
        finish(null);
      }}
      // Only the focused window hears the keyboard, and with a surface on
      // every monitor the focused one is rarely the one under the pointer
      // — which is where Escape is expected to work.
      onPointerEnter={() => {
        captureSurfaceFocus().catch(() => {});
      }}
    >
      {surface && (
        <img
          ref={image}
          className="capture-frozen"
          src={surface.image}
          alt=""
          draggable={false}
          // Shown only once the frame is painted: a window that appears
          // before its image is a white flash over what is being captured.
          onLoad={() => captureSurfaceReady().catch(() => {})}
        />
      )}

      {/* Dimming: the whole screen while nothing is selected, and
          everything outside the rectangle once something is. The selection
          punches its own hole with an outsized shadow rather than four
          divs that have to agree with each other. */}
      {!box && <div className="capture-dim" />}
      {box && (
        <div className="capture-selection" style={box}>
          {pixels && pixels.width > 0 && (
            <span className="capture-size">
              {pixels.width} × {pixels.height}
            </span>
          )}
        </div>
      )}

      {!drag && (
        <div className="capture-hint">
          {s("app", surface?.recording ? "capture_hint_record" : "capture_hint")}
          {/* Which of the dimmed screens this is. Only worth saying when
              there is more than one, where it answers a real question. */}
          {surface && surface.screens > 1 && (
            <span className="capture-screen">
              {s("app", "capture_screen_badge", {
                n: surface.index + 1,
                total: surface.screens,
              })}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
