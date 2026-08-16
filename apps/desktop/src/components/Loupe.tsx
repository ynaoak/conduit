import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/** One frame pushed from Rust (see src-tauri/src/loupe.rs) */
interface Frame {
  pixels: string[];
  grid: number;
  center: string;
  ink: string;
}

/**
 * The magnifier shown while picking a color.
 *
 * Rendered into its own window (`index.html#loupe`), which never takes
 * focus and ignores the pointer — so this component only ever draws. It
 * has no way to read the screen itself and does not try: every frame
 * arrives as an event.
 */
export function Loupe() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [frame, setFrame] = useState<Frame | null>(null);

  useEffect(() => {
    const unlisten = listen<Frame>("conduit://loupe", (event) => setFrame(event.payload));
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !frame) return;
    const context = canvas.getContext("2d");
    if (!context) return;

    // One canvas pixel per sampled pixel, scaled up by CSS with
    // image-rendering: pixelated — so the cells stay square and crisp
    // instead of being smoothed into a blur.
    canvas.width = frame.grid;
    canvas.height = frame.grid;
    for (let i = 0; i < frame.pixels.length; i++) {
      context.fillStyle = frame.pixels[i];
      context.fillRect(i % frame.grid, Math.floor(i / frame.grid), 1, 1);
    }
  }, [frame]);

  const middle = frame ? (frame.grid - 1) / 2 : 0;
  const cell = frame ? 100 / frame.grid : 0;

  return (
    <div className="loupe">
      <div className="loupe-glass">
        <canvas ref={canvasRef} className="loupe-canvas" />
        {frame && (
          // The crosshair marks the pixel a click would take. Drawn over
          // the canvas rather than into it so it never becomes one of the
          // sampled colors.
          <div
            className="loupe-target"
            style={{
              left: `${middle * cell}%`,
              top: `${middle * cell}%`,
              width: `${cell}%`,
              height: `${cell}%`,
            }}
          />
        )}
      </div>
      <div
        className="loupe-label"
        style={{ background: frame?.center ?? "#000", color: frame?.ink ?? "#fff" }}
      >
        {frame?.center ?? "…"}
      </div>
    </div>
  );
}
