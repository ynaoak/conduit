import type { Keybindings } from "./types";

/** Mirrors KeybindingsConfig::default() on the Rust side */
export const DEFAULT_KEYBINDINGS: Keybindings = {
  move_up: ["ArrowUp", "Ctrl+P"],
  move_down: ["ArrowDown", "Ctrl+N"],
  execute: ["Enter"],
  // Bare "Shift" is a tap: press and release without another key
  action_panel: ["Shift", "Ctrl+K"],
  close: ["Escape"],
  toggle_pin: ["Ctrl+D"],
  manage_view: ["Ctrl+,"],
  tab_next: ["Tab", "ArrowRight", "Ctrl+Tab"],
  tab_prev: ["Shift+Tab", "ArrowLeft", "Ctrl+Shift+Tab"],
};

interface ParsedChord {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
  key: string;
}

function normalizeKey(key: string): string {
  switch (key) {
    case "esc":
      return "escape";
    case "space":
      return " ";
    case "return":
      return "enter";
    case "up":
      return "arrowup";
    case "down":
      return "arrowdown";
    case "left":
      return "arrowleft";
    case "right":
      return "arrowright";
    default:
      return key;
  }
}

function parseChord(chord: string): ParsedChord {
  const parsed: ParsedChord = {
    ctrl: false,
    alt: false,
    shift: false,
    meta: false,
    key: "",
  };
  for (const part of chord.split("+")) {
    const low = part.trim().toLowerCase();
    if (low === "ctrl" || low === "control") parsed.ctrl = true;
    else if (low === "alt") parsed.alt = true;
    else if (low === "shift") parsed.shift = true;
    else if (low === "win" || low === "super" || low === "meta" || low === "cmd")
      parsed.meta = true;
    else parsed.key = normalizeKey(low);
  }
  return parsed;
}

/** True if the keyboard event matches any of the configured chords */
export function matchChord(e: KeyboardEvent, chords: string[]): boolean {
  return chords.some((chord) => {
    const p = parseChord(chord);
    return (
      p.key !== "" &&
      e.ctrlKey === p.ctrl &&
      e.altKey === p.alt &&
      e.shiftKey === p.shift &&
      e.metaKey === p.meta &&
      e.key.toLowerCase() === p.key
    );
  });
}

/** Convert a keydown event into a chord string like "Ctrl+Shift+K".
 *  Returns null for bare modifier presses. */
export function eventToChord(e: KeyboardEvent): string | null {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Win");
  const key =
    e.key === " " ? "Space" : e.key.length === 1 ? e.key.toUpperCase() : e.key;
  parts.push(key);
  return parts.join("+");
}

/** Map a KeyboardEvent's main key to the config `key` format understood by
 *  the Rust global-shortcut parser (Space / A-Z / F1-F12 / Enter / Tab /
 *  Escape / Backspace). Returns null for unsupported keys. */
function toHotkeyCode(e: KeyboardEvent): string | null {
  if (e.key === " ") return "Space";
  if (/^[a-z]$/i.test(e.key)) return e.key.toUpperCase();
  if (/^F([1-9]|1[0-2])$/.test(e.key)) return e.key;
  if (["Enter", "Tab", "Escape", "Backspace"].includes(e.key)) return e.key;
  return null;
}

/** Convert a keydown event into a global hotkey {modifier, key}. Requires at
 *  least one modifier + a supported key; returns null otherwise (so a bare
 *  modifier press keeps the recorder waiting). */
export function eventToHotkey(
  e: KeyboardEvent,
): { modifier: string; key: string } | null {
  const key = toHotkeyCode(e);
  if (!key) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Win");
  if (mods.length === 0) return null;
  return { modifier: mods.join("+"), key };
}

/** Compact display label for the first chord, e.g. "Escape" -> "Esc" */
export function chordLabel(chords: string[]): string {
  const chord = chords[0] ?? "";
  return chord
    .split("+")
    .map((part) => {
      const low = part.trim().toLowerCase();
      switch (low) {
        case "enter":
        case "return":
          return "⏎";
        case "escape":
        case "esc":
          return "Esc";
        case "arrowup":
        case "up":
          return "↑";
        case "arrowdown":
        case "down":
          return "↓";
        case "space":
          return "Space";
        default:
          return part.trim().length === 1
            ? part.trim().toUpperCase()
            : part.trim();
      }
    })
    .join("+");
}
