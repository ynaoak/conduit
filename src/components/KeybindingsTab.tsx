import { useEffect, useRef, useState, type CSSProperties } from "react";
import { openConfigFile, restartApp, saveConfig } from "../lib/ipc";
import {
  DEFAULT_KEYBINDINGS,
  eventToChord,
  eventToHotkey,
  matchChord,
} from "../lib/keys";
import type { AppConfig, Keybindings } from "../lib/types";

interface KeybindingsTabProps {
  config: AppConfig | null;
  onConfigSaved: (config: AppConfig) => void;
  /** Notify the parent while a key is being recorded so it pauses its own key handling */
  onRecordingChange: (recording: boolean) => void;
  /** Applied (saved) bindings, used for row navigation matching */
  keybindings: Keybindings;
}

const ACTION_ROWS: { key: keyof Keybindings; label: string }[] = [
  { key: "move_up", label: "上へ移動" },
  { key: "move_down", label: "下へ移動" },
  { key: "execute", label: "実行" },
  { key: "action_panel", label: "アクションパネル" },
  { key: "close", label: "閉じる" },
  { key: "toggle_pin", label: "ピン留め / 解除" },
  { key: "manage_view", label: "管理ビューを開く / 閉じる" },
  { key: "tab_next", label: "次のタブ" },
  { key: "tab_prev", label: "前のタブ" },
];

const rowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "8px",
  padding: "7px 8px",
  borderBottom: "1px solid var(--border-color)",
};

const labelStyle: CSSProperties = {
  width: "180px",
  fontSize: "13px",
  color: "var(--text-primary)",
  flexShrink: 0,
};

const chipStyle: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: "6px",
  background: "var(--bg-primary)",
  border: "1px solid var(--border-color)",
  borderRadius: "6px",
  fontSize: "12px",
  color: "var(--text-primary)",
  padding: "2px 8px",
};

const chipRemoveStyle: CSSProperties = {
  background: "transparent",
  border: "none",
  color: "var(--text-secondary)",
  cursor: "pointer",
  fontSize: "11px",
  padding: 0,
};

const addButtonStyle: CSSProperties = {
  background: "transparent",
  border: "1px dashed var(--border-color)",
  borderRadius: "6px",
  color: "var(--text-secondary)",
  fontSize: "11px",
  padding: "2px 10px",
  cursor: "pointer",
};

const footerButtonStyle: CSSProperties = {
  background: "var(--bg-selected)",
  border: "1px solid var(--border-color)",
  borderRadius: "6px",
  color: "var(--text-primary)",
  fontSize: "12px",
  padding: "6px 16px",
  cursor: "pointer",
};

const ghostButtonStyle: CSSProperties = {
  ...footerButtonStyle,
  background: "transparent",
  color: "var(--text-secondary)",
};

export function KeybindingsTab({
  config,
  onConfigSaved,
  onRecordingChange,
  keybindings,
}: KeybindingsTabProps) {
  const [bindings, setBindings] = useState<Keybindings>(() => ({
    ...DEFAULT_KEYBINDINGS,
    ...config?.keybindings,
  }));
  const [recordingFor, setRecordingFor] = useState<keyof Keybindings | null>(null);
  const [selectedRow, setSelectedRow] = useState(0);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);
  // Startup-only global activation, edited locally then saved + restarted
  const [hotkeyModifier, setHotkeyModifier] = useState(
    config?.hotkey.modifier ?? "Alt",
  );
  const [hotkeyKey, setHotkeyKey] = useState(config?.hotkey.key ?? "Space");
  const [doubleTap, setDoubleTap] = useState(config?.hotkey.double_tap ?? "Ctrl");
  const [recordingHotkey, setRecordingHotkey] = useState(false);
  const savedTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const rowRefs = useRef<(HTMLDivElement | null)[]>([]);

  // Pause the parent's key handling while recording anything
  useEffect(() => {
    onRecordingChange(recordingFor !== null || recordingHotkey);
  }, [recordingFor, recordingHotkey, onRecordingChange]);

  // Record the global hotkey: needs modifier(s) + a supported key
  useEffect(() => {
    if (!recordingHotkey) return;
    const handler = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecordingHotkey(false);
        return;
      }
      const hk = eventToHotkey(e);
      if (!hk) return; // bare modifier or unsupported key — keep waiting
      setHotkeyModifier(hk.modifier);
      setHotkeyKey(hk.key);
      setRecordingHotkey(false);
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [recordingHotkey]);

  // Up/down moves the row selection; execute starts recording for that row.
  // Paused while a key is being recorded.
  useEffect(() => {
    if (recordingHotkey) return;
    if (recordingFor !== null) return;
    const handler = (e: KeyboardEvent) => {
      if (matchChord(e, keybindings.move_down)) {
        e.preventDefault();
        setSelectedRow((i) => Math.min(i + 1, ACTION_ROWS.length - 1));
      } else if (matchChord(e, keybindings.move_up)) {
        e.preventDefault();
        setSelectedRow((i) => Math.max(i - 1, 0));
      } else if (matchChord(e, keybindings.execute)) {
        e.preventDefault();
        setRecordingFor(ACTION_ROWS[selectedRow].key);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [recordingFor, recordingHotkey, selectedRow, keybindings]);

  useEffect(() => {
    rowRefs.current[selectedRow]?.scrollIntoView({ block: "nearest" });
  }, [selectedRow]);

  // Capture the next keypress while recording. A bare modifier pressed and
  // released on its own is recorded as a tap chord (e.g. "Shift").
  useEffect(() => {
    if (recordingFor === null) return;

    const MODIFIER_NAMES: Record<string, string> = {
      Shift: "Shift",
      Control: "Ctrl",
      Alt: "Alt",
      Meta: "Win",
    };
    let pendingModifier: string | null = null;

    const record = (chord: string) => {
      setBindings((prev) => {
        const existing = prev[recordingFor];
        if (existing.includes(chord)) return prev;
        return { ...prev, [recordingFor]: [...existing, chord] };
      });
      setRecordingFor(null);
    };

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecordingFor(null);
        return;
      }
      const modifier = MODIFIER_NAMES[e.key];
      if (modifier) {
        // Might become a bare-modifier tap; a second modifier cancels it
        pendingModifier = pendingModifier === null ? modifier : "cancelled";
        return;
      }
      pendingModifier = "cancelled";
      const chord = eventToChord(e);
      if (chord) record(chord);
    };

    const onKeyUp = (e: KeyboardEvent) => {
      const modifier = MODIFIER_NAMES[e.key];
      if (!modifier) return;
      e.preventDefault();
      e.stopPropagation();
      if (pendingModifier === modifier) record(modifier);
    };

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
    };
  }, [recordingFor, onRecordingChange]);

  const removeChord = (action: keyof Keybindings, chord: string) => {
    setBindings((prev) => ({
      ...prev,
      [action]: prev[action].filter((c) => c !== chord),
    }));
  };

  const showSaved = (message: string) => {
    clearTimeout(savedTimerRef.current);
    setSavedMessage(message);
    savedTimerRef.current = setTimeout(() => setSavedMessage(null), 2500);
  };

  const save = () => {
    if (!config) return;
    const next = { ...config, keybindings: bindings };
    saveConfig(next)
      .then(() => {
        onConfigSaved(next);
        showSaved("保存しました");
      })
      .catch((err) => showSaved(`保存に失敗: ${err}`));
  };

  // Global activation (hotkey + double-tap) is read only at startup, so it
  // must be saved then applied with a restart
  const saveGlobalAndRestart = () => {
    if (!config) return;
    const next = {
      ...config,
      hotkey: {
        ...config.hotkey,
        modifier: hotkeyModifier,
        key: hotkeyKey,
        double_tap: doubleTap,
      },
    };
    saveConfig(next)
      .then(() => {
        onConfigSaved(next);
        restartApp().catch(console.error);
      })
      .catch((err) => showSaved(`保存に失敗: ${err}`));
  };

  if (!config) {
    return (
      <div style={{ padding: "24px", fontSize: "13px", color: "var(--text-secondary)" }}>
        設定を読み込めませんでした。アプリを再起動してみてください。
      </div>
    );
  }

  return (
    <div>
      <div style={{ fontSize: "11px", color: "var(--text-secondary)", padding: "0 8px 6px" }}>
        ↑↓ で行を選択 / Enter で選択行にキーを追加
      </div>
      {ACTION_ROWS.map(({ key, label }, index) => (
        <div
          key={key}
          ref={(el) => {
            rowRefs.current[index] = el;
          }}
          onMouseEnter={() => setSelectedRow(index)}
          style={{
            ...rowStyle,
            ...(index === selectedRow
              ? { background: "var(--bg-hover)", borderRadius: "6px" }
              : {}),
          }}
        >
          <span style={labelStyle}>{label}</span>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "6px", alignItems: "center" }}>
            {bindings[key].map((chord) => (
              <span key={chord} style={chipStyle}>
                {chord}
                <button
                  style={chipRemoveStyle}
                  onClick={() => removeChord(key, chord)}
                  aria-label={`${chord} を削除`}
                >
                  ✕
                </button>
              </span>
            ))}
            <button
              style={{
                ...addButtonStyle,
                ...(recordingFor === key
                  ? { borderColor: "var(--text-accent)", color: "var(--text-accent)" }
                  : {}),
              }}
              onClick={() => setRecordingFor(recordingFor === key ? null : key)}
            >
              {recordingFor === key ? "キーを入力... (Esc で中止)" : "+ キーを追加"}
            </button>
          </div>
        </div>
      ))}

      <div style={{ display: "flex", alignItems: "center", gap: "8px", padding: "12px 8px" }}>
        <button style={footerButtonStyle} onClick={save}>
          保存
        </button>
        <button
          style={ghostButtonStyle}
          onClick={() => setBindings({ ...DEFAULT_KEYBINDINGS })}
        >
          デフォルトに戻す
        </button>
        {savedMessage && (
          <span style={{ fontSize: "12px", color: "var(--text-accent)" }}>{savedMessage}</span>
        )}
      </div>

      <div
        style={{
          margin: "4px 8px",
          padding: "10px 12px",
          background: "var(--bg-secondary)",
          border: "1px solid var(--border-color)",
          borderRadius: "8px",
          fontSize: "12px",
          color: "var(--text-secondary)",
        }}
      >
        <div style={{ color: "var(--text-primary)", fontWeight: 600, marginBottom: "8px" }}>
          グローバル起動{" "}
          <span style={{ opacity: 0.7, fontWeight: 400 }}>（変更は再起動で反映）</span>
        </div>

        <div style={rowStyle}>
          <span style={labelStyle}>ホットキー</span>
          <span style={chipStyle}>
            {hotkeyModifier}+{hotkeyKey}
          </span>
          <button
            style={{
              ...addButtonStyle,
              ...(recordingHotkey
                ? { borderColor: "var(--text-accent)", color: "var(--text-accent)" }
                : {}),
            }}
            onClick={() => setRecordingHotkey((v) => !v)}
          >
            {recordingHotkey ? "キーを入力... (Esc で中止)" : "変更"}
          </button>
        </div>

        <div style={rowStyle}>
          <span style={labelStyle}>2連打で起動</span>
          <select
            value={doubleTap}
            onChange={(e) => setDoubleTap(e.target.value)}
            style={{
              background: "var(--bg-primary)",
              color: "var(--text-primary)",
              border: "1px solid var(--border-color)",
              borderRadius: "6px",
              padding: "3px 8px",
              fontSize: "12px",
            }}
          >
            <option value="">なし</option>
            <option value="Ctrl">Ctrl</option>
            <option value="Alt">Alt</option>
            <option value="Shift">Shift</option>
            <option value="Win">Win</option>
          </select>
          <span style={{ opacity: 0.7 }}>（PowerToys 等と衝突する場合は なし に）</span>
        </div>

        <div style={{ display: "flex", gap: "8px", marginTop: "10px" }}>
          <button
            style={footerButtonStyle}
            onClick={saveGlobalAndRestart}
            title="起動キーを保存し、アプリを再起動して反映"
          >
            🔄 保存して再起動
          </button>
          <button style={ghostButtonStyle} onClick={() => openConfigFile().catch(console.error)}>
            config.json を開く
          </button>
        </div>
      </div>
    </div>
  );
}
