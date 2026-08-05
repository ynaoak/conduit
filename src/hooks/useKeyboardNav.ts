import { useState, useEffect, useCallback, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { matchChord } from "../lib/keys";
import type { Keybindings, SearchResult } from "../lib/types";

/** Shell-handled action id: pin or unpin depending on current state */
export const TOGGLE_PIN_ACTION = "__toggle_pin";

/** KeyboardEvent.key -> chord name for bare-modifier taps */
const MODIFIER_NAMES: Record<string, string> = {
  Shift: "Shift",
  Control: "Ctrl",
  Alt: "Alt",
  Meta: "Win",
};

/** Unmodified horizontal arrows inside a text input move the caret and must
 *  not be captured as screen-cycling keys. */
function isCaretMovement(e: KeyboardEvent): boolean {
  return (
    (e.key === "ArrowLeft" || e.key === "ArrowRight") &&
    !e.ctrlKey &&
    !e.altKey &&
    !e.metaKey &&
    (e.target as HTMLElement | null)?.tagName === "INPUT"
  );
}

export function useKeyboardNav(
  results: SearchResult[],
  onExecute: (result: SearchResult, actionId?: string) => void,
  bindings: Keybindings,
  enabled: boolean = true,
  onOpenManage?: () => void,
  onCycleScreen?: (direction: 1 | -1) => void,
) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [panelOpen, setPanelOpen] = useState(false);
  const [actionIndex, setActionIndex] = useState(0);
  /** Bare-modifier tap tracking: modifier name, "cancelled", or null */
  const pendingTapRef = useRef<string | null>(null);

  // Reset selection and panel when results change
  useEffect(() => {
    setSelectedIndex(0);
    setPanelOpen(false);
    setActionIndex(0);
  }, [results]);

  const closePanel = useCallback(() => {
    setPanelOpen(false);
    setActionIndex(0);
  }, []);

  useEffect(() => {
    if (!enabled) return;

    // Shared between keydown chords and keyup modifier taps
    const dispatch = (
      matches: (chords: string[]) => boolean,
      e: KeyboardEvent,
    ) => {
      const selected = results[selectedIndex];

      // Action panel open: navigate and run actions
      if (panelOpen && selected) {
        if (matches(bindings.move_down)) {
          e.preventDefault();
          setActionIndex((i) => Math.min(i + 1, selected.actions.length - 1));
        } else if (matches(bindings.move_up)) {
          e.preventDefault();
          setActionIndex((i) => Math.max(i - 1, 0));
        } else if (matches(bindings.execute)) {
          e.preventDefault();
          onExecute(selected, selected.actions[actionIndex]?.id);
          closePanel();
        } else if (matches(bindings.close) || matches(bindings.action_panel)) {
          // "close" dismisses the panel, not the window
          e.preventDefault();
          closePanel();
        }
        return;
      }

      if (onOpenManage && matches(bindings.manage_view)) {
        e.preventDefault();
        onOpenManage();
        return;
      }

      // Pin/unpin the highlighted result without opening the action panel
      if (selected && matches(bindings.toggle_pin)) {
        e.preventDefault();
        onExecute(selected, TOGGLE_PIN_ACTION);
        return;
      }

      if (matches(bindings.action_panel)) {
        e.preventDefault();
        // Single-action results have nothing extra to show
        if (selected && selected.actions.length > 1) {
          setPanelOpen(true);
          setActionIndex(0);
        }
        return;
      }

      // Cycle screens: search -> workflows -> launcher -> keybindings -> ...
      if (onCycleScreen && !isCaretMovement(e)) {
        if (matches(bindings.tab_next)) {
          e.preventDefault();
          onCycleScreen(1);
          return;
        }
        if (matches(bindings.tab_prev)) {
          e.preventDefault();
          onCycleScreen(-1);
          return;
        }
      }

      if (matches(bindings.move_down)) {
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, results.length - 1));
      } else if (matches(bindings.move_up)) {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
      } else if (matches(bindings.execute)) {
        e.preventDefault();
        if (selected) {
          onExecute(selected);
        }
      } else if (matches(bindings.close)) {
        e.preventDefault();
        getCurrentWindow().hide().catch(console.error);
      }
    };

    const onKeyDown = (e: KeyboardEvent) => {
      const modifier = MODIFIER_NAMES[e.key];
      if (modifier) {
        if (e.repeat) return;
        // First modifier starts a potential tap; a second one cancels it
        pendingTapRef.current =
          pendingTapRef.current === null ? modifier : "cancelled";
        return;
      }
      // Any real key cancels the tap (e.g. Shift+A while typing)
      pendingTapRef.current = "cancelled";
      dispatch((chords) => matchChord(e, chords), e);
    };

    const onKeyUp = (e: KeyboardEvent) => {
      const modifier = MODIFIER_NAMES[e.key];
      const noModifiersHeld =
        !e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey;

      if (!modifier) {
        if (noModifiersHeld) pendingTapRef.current = null;
        return;
      }

      const tapped = pendingTapRef.current === modifier;
      if (noModifiersHeld) pendingTapRef.current = null;
      if (tapped) {
        // Clean press-and-release of a single modifier: fire bindings that
        // name it verbatim (e.g. "Shift" opens the action panel)
        dispatch((chords) => chords.includes(modifier), e);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, [results, selectedIndex, panelOpen, actionIndex, onExecute, closePanel, bindings, enabled, onOpenManage, onCycleScreen]);

  return {
    selectedIndex,
    setSelectedIndex,
    panelOpen,
    actionIndex,
    setActionIndex,
    closePanel,
  };
}
