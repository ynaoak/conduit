import { useEffect, useState } from "react";
import { recordingState, resolvedLanguage, stopRecording } from "../lib/ipc";
import { makeStrings, resolveLanguage } from "../lib/i18n";

function clock(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

/**
 * The recorder's window: a clock and a stop button, floating over the
 * screen being recorded (see src-tauri/src/capture/record.rs).
 *
 * It exists so a recording is never invisible. The window is closed by
 * Rust when the recording ends — including when it ends by itself at the
 * cap — so this component never closes anything.
 */
export function Recorder() {
  const [seconds, setSeconds] = useState(0);
  const [max, setMax] = useState(0);
  const [stopping, setStopping] = useState(false);
  const [lang, setLang] = useState(() => resolveLanguage(undefined));

  const s = makeStrings(lang);

  useEffect(() => {
    // Counted from a timestamp rather than by adding one per tick: an
    // interval that is late (and it will be) would run the clock slow.
    const started = Date.now();
    const id = window.setInterval(
      () => setSeconds(Math.floor((Date.now() - started) / 1000)),
      250,
    );
    resolvedLanguage()
      .then(setLang)
      .catch(() => {});
    recordingState()
      .then((state) => setMax(state.max_seconds))
      .catch(() => {});
    return () => window.clearInterval(id);
  }, []);

  const stop = () => {
    if (stopping) return;
    setStopping(true);
    // The result is the saved capture, which the history already has;
    // this window is about to be closed either way.
    stopRecording().catch(() => setStopping(false));
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") stop();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  return (
    <div className="recorder">
      <span className="recorder-dot" aria-hidden="true" />
      <span className="recorder-clock">{clock(seconds)}</span>
      <span className="recorder-label">
        {max > 0 ? `${s("app", "recorder_hint")} / ${clock(max)}` : s("app", "recorder_hint")}
      </span>
      <button type="button" className="recorder-stop" onClick={stop} disabled={stopping}>
        {s("app", "recorder_stop")}
      </button>
    </div>
  );
}
