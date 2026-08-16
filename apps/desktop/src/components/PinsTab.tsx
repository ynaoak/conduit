import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { listPins, movePin, removePin } from "../lib/ipc";
import { matchChord } from "../lib/keys";
import { MaterialIcon } from "./MaterialIcon";
import { useStrings } from "../lib/i18n";
import type { Keybindings, PinnedItem, SearchResult } from "../lib/types";
import { ResultItem } from "./ResultItem";

interface PinsTabProps {
  onExecute: (result: SearchResult, actionId?: string) => void;
  keybindings: Keybindings;
  onContextMenu?: (result: SearchResult, x: number, y: number) => void;
}

const emptyStyle: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  height: "70%",
  color: "var(--text-secondary)",
  fontSize: "13px",
  gap: "8px",
};

const hintStyle: CSSProperties = {
  fontSize: "11px",
  color: "var(--text-secondary)",
  padding: "0 8px 6px",
};

/** A pin carries the original result's fields, so it can be replayed as-is */
function toSearchResult(pin: PinnedItem): SearchResult {
  return {
    id: pin.result_id,
    plugin_id: pin.plugin_id,
    title: pin.title,
    subtitle: pin.subtitle,
    icon: pin.icon,
    score: 1,
    actions: pin.actions,
  };
}

export function PinsTab({ onExecute, keybindings, onContextMenu }: PinsTabProps) {
  const s = useStrings();
  const [pins, setPins] = useState<PinnedItem[] | null>(null);
  const [selected, setSelected] = useState(0);
  const itemRefs = useRef<(HTMLDivElement | null)[]>([]);

  const reload = useCallback(() => {
    listPins()
      .then(setPins)
      .catch(() => setPins([]));
  }, []);

  useEffect(reload, [reload]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!pins || pins.length === 0) return;
      const pin = pins[selected];

      // Ctrl + up/down reorders instead of moving the selection
      const reorder = e.ctrlKey;
      if (matchChord(e, keybindings.move_down) || (reorder && e.key === "ArrowDown")) {
        e.preventDefault();
        if (reorder && pin) {
          const target = Math.min(selected + 1, pins.length - 1);
          movePin(pin.result_id, target).then(reload).catch(console.error);
          setSelected(target);
        } else {
          setSelected((i) => Math.min(i + 1, pins.length - 1));
        }
      } else if (matchChord(e, keybindings.move_up) || (reorder && e.key === "ArrowUp")) {
        e.preventDefault();
        if (reorder && pin) {
          const target = Math.max(selected - 1, 0);
          movePin(pin.result_id, target).then(reload).catch(console.error);
          setSelected(target);
        } else {
          setSelected((i) => Math.max(i - 1, 0));
        }
      } else if (matchChord(e, keybindings.execute)) {
        e.preventDefault();
        if (pin) onExecute(toSearchResult(pin));
      } else if (e.key === "Delete" || matchChord(e, keybindings.toggle_pin)) {
        // Delete or the pin toggle (Ctrl+D) unpins from here
        e.preventDefault();
        if (pin) {
          removePin(pin.result_id).then(reload).catch(console.error);
          setSelected((i) => Math.max(0, Math.min(i, pins.length - 2)));
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [pins, selected, keybindings, onExecute, reload]);

  useEffect(() => {
    itemRefs.current[selected]?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  if (pins && pins.length === 0) {
    return (
      <div style={emptyStyle}>
        <MaterialIcon name="push_pin" size={36} color="var(--text-secondary)" style={{ opacity: 0.6 }} />
        <span>{s("manage", "pins_empty")}</span>
        <span style={{ fontSize: "11px", opacity: 0.8 }}>
          {s("manage", "pins_hint_full", { chord: "Ctrl+D" })}
        </span>
      </div>
    );
  }

  return (
    <>
      {pins && (
        <div style={hintStyle}>
          {s("manage", "pins_summary", { count: pins.length })}
        </div>
      )}
      {pins?.map((pin, index) => (
        <div
          key={pin.result_id}
          ref={(el) => {
            itemRefs.current[index] = el;
          }}
        >
          <ResultItem
            result={toSearchResult(pin)}
            isSelected={index === selected}
            onClick={() => {
              setSelected(index);
              onExecute(toSearchResult(pin));
            }}
            onContextMenu={(r, x, y) => {
              setSelected(index);
              onContextMenu?.(r, x, y);
            }}
          />
        </div>
      ))}
    </>
  );
}
