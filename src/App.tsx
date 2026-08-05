import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { SearchBar } from "./components/SearchBar";
import { KeywordChips } from "./components/KeywordChips";
import { ResultList } from "./components/ResultList";
import { StatusBar } from "./components/StatusBar";
import { ActionPanel } from "./components/ActionPanel";
import { ContextMenu } from "./components/ContextMenu";
import { ManageView } from "./components/ManageView";
import { ScreenTabs } from "./components/ScreenTabs";
import { useSearch } from "./hooks/useSearch";
import { useKeyboardNav, TOGGLE_PIN_ACTION } from "./hooks/useKeyboardNav";
import { useTheme } from "./hooks/useTheme";
import {
  addPin,
  executeAction,
  getConfig,
  importWorkflow,
  listPins,
  removePin,
  setPinned,
} from "./lib/ipc";
import { DEFAULT_KEYBINDINGS } from "./lib/keys";
import type { AppConfig, SearchResult } from "./lib/types";
import type { Screen } from "./components/ScreenTabs";
import "./styles/global.css";

/** Cycle order: search input, then the manage tabs */
const SCREEN_ORDER: Screen[] = [
  "search",
  "pins",
  "workflows",
  "launcher",
  "keybindings",
];

function focusSearchInput() {
  setTimeout(() => {
    (document.getElementById("search-input") as HTMLInputElement)?.focus();
  }, 0);
}

/** Actions whose result lives on the clipboard — keep the window open and
 *  confirm with a toast instead of silently hiding */
const COPY_ACTIONS = new Set(["copy", "copy-path", "copy-url", "copy-pid"]);

/** Synthetic action ids handled by the shell, not by any plugin */
const PIN_ACTION = "__pin";
const UNPIN_ACTION = "__unpin";

function App() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const [screen, setScreen] = useState<Screen>("search");
  const [pinned, setPinnedState] = useState(false);
  /** result_id set of pinned favorites, for the pin/unpin action label */
  const [pinnedIds, setPinnedIds] = useState<Set<string>>(new Set());
  const [contextMenu, setContextMenu] = useState<{
    result: SearchResult;
    x: number;
    y: number;
  } | null>(null);
  const toastTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const { query, setQuery, results: rawResults, isLoading, refresh } = useSearch();

  // Every result gains a pin/unpin action — pinning is handled by the shell,
  // so no plugin needs to know about it
  const withPinAction = useCallback(
    (result: SearchResult): SearchResult => ({
      ...result,
      actions: [
        ...result.actions.filter(
          (a) => a.id !== PIN_ACTION && a.id !== UNPIN_ACTION,
        ),
        pinnedIds.has(result.id)
          ? { id: UNPIN_ACTION, title: "📌 ピン留めを解除", shortcut: null }
          : { id: PIN_ACTION, title: "📌 ピン留め", shortcut: null },
      ],
    }),
    [pinnedIds],
  );

  const results = useMemo(
    () => rawResults.map(withPinAction),
    [rawResults, withPinAction],
  );

  const openContextMenu = useCallback(
    (result: SearchResult, x: number, y: number) =>
      setContextMenu({ result: withPinAction(result), x, y }),
    [withPinAction],
  );

  const showToast = useCallback((message: string) => {
    clearTimeout(toastTimerRef.current);
    setToast(message);
    toastTimerRef.current = setTimeout(() => setToast(null), 3000);
  }, []);

  // Execute a result action (Enter key, mouse click, or action panel)
  const executeResult = useCallback(
    (result: SearchResult, actionId?: string) => {
      const action = actionId ?? result.actions[0]?.id;

      // Pinning is a shell concern — no plugin knows about it.
      // TOGGLE_PIN_ACTION (Ctrl+D) resolves against the current pin state.
      const pinAction =
        action === TOGGLE_PIN_ACTION
          ? pinnedIds.has(result.id)
            ? UNPIN_ACTION
            : PIN_ACTION
          : action;

      if (pinAction === PIN_ACTION || pinAction === UNPIN_ACTION) {
        const task =
          pinAction === PIN_ACTION
            ? addPin({
                plugin_id: result.plugin_id,
                result_id: result.id,
                title: result.title,
                subtitle: result.subtitle,
                icon: result.icon,
                actions: result.actions.filter(
                  (a) => a.id !== PIN_ACTION && a.id !== UNPIN_ACTION,
                ),
              })
            : removePin(result.id);
        task
          .then(() => {
            listPins()
              .then((items) => setPinnedIds(new Set(items.map((p) => p.result_id))))
              .catch(console.error);
            showToast(
              pinAction === PIN_ACTION
                ? "📌 ピン留めしました"
                : "ピン留めを解除しました",
            );
          })
          .catch((err) => showToast(`失敗: ${err}`));
        return;
      }

      // Deleting a clipboard entry keeps the window open and refreshes
      if (result.plugin_id === "conduit.clipboard-history" && action === "delete") {
        executeAction(result.plugin_id, result.id, action)
          .then(refresh)
          .catch(console.error);
        return;
      }

      // Killing a process keeps the window open: the refreshed list is the
      // confirmation that the port was freed, and failures (protected or
      // access-denied processes) must be surfaced rather than swallowed
      if (result.plugin_id === "conduit.process-monitor" && action === "kill") {
        executeAction(result.plugin_id, result.id, action)
          .then(() => {
            showToast(`✅ ${result.title} を終了しました`);
            refresh();
          })
          .catch((err) => showToast(`終了に失敗: ${err}`));
        return;
      }

      // Copy actions: stay open and confirm — hiding silently gives no
      // feedback that the clipboard now holds the value
      if (action && COPY_ACTIONS.has(action)) {
        executeAction(result.plugin_id, result.id, action)
          .then(() => showToast("📋 コピーしました"))
          .catch((err) => showToast(`コピーに失敗: ${err}`));
        return;
      }

      executeAction(result.plugin_id, result.id, action).catch(console.error);
      getCurrentWindow().hide().catch(console.error);
    },
    [refresh, showToast, pinnedIds],
  );

  const keybindings = config?.keybindings
    ? { ...DEFAULT_KEYBINDINGS, ...config.keybindings }
    : DEFAULT_KEYBINDINGS;

  const openManage = useCallback(() => setScreen("workflows"), []);

  // Tab / Shift+Tab cycle: search -> workflows -> launcher -> keybindings -> ...
  const cycleScreen = useCallback((direction: 1 | -1) => {
    setScreen((current) => {
      const index = SCREEN_ORDER.indexOf(current);
      const next =
        SCREEN_ORDER[
          (index + direction + SCREEN_ORDER.length) % SCREEN_ORDER.length
        ];
      if (next === "search") focusSearchInput();
      return next;
    });
  }, []);

  const {
    selectedIndex,
    setSelectedIndex,
    panelOpen,
    actionIndex,
    setActionIndex,
    closePanel,
  } = useKeyboardNav(
    results,
    executeResult,
    keybindings,
    screen === "search",
    openManage,
    cycleScreen,
  );

  const backToSearch = useCallback(() => {
    setScreen("search");
    // Restore focus to the search input when returning
    focusSearchInput();
  }, []);

  const togglePin = useCallback(() => {
    setPinnedState((current) => {
      const next = !current;
      setPinned(next).catch(console.error);
      return next;
    });
  }, []);

  // Suppress the WebView's built-in context menu everywhere except the
  // search input, where the native copy/paste menu is still useful
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.id === "search-input") return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", handler);
    return () => window.removeEventListener("contextmenu", handler);
  }, []);

  // Load config and pinned ids on mount
  useEffect(() => {
    getConfig().then(setConfig).catch(console.error);
    listPins()
      .then((items) => setPinnedIds(new Set(items.map((p) => p.result_id))))
      .catch(console.error);
  }, []);

  // Apply theme from config
  useTheme(config);

  // Workflow zip import via drag & drop
  useEffect(() => {
    let unlisten: Promise<() => void> | null = null;
    try {
      unlisten = getCurrentWebview().onDragDropEvent(async (event) => {
        if (event.payload.type === "over") {
          setIsDragOver(true);
          return;
        }
        setIsDragOver(false);
        if (event.payload.type !== "drop") return;

        const zips = event.payload.paths.filter((p) =>
          p.toLowerCase().endsWith(".zip"),
        );
        if (zips.length === 0) {
          showToast("ワークフローは .zip ファイルでドロップしてください");
          return;
        }
        for (const path of zips) {
          try {
            const name = await importWorkflow(path);
            showToast(`ワークフロー「${name}」をインポートしました`);
          } catch (err) {
            showToast(`インポート失敗: ${err}`);
          }
        }
      });
    } catch {
      // Not running inside Tauri (e.g. plain-browser preview) — no drag & drop
    }
    return () => {
      unlisten?.then((fn) => fn()).catch(() => {});
    };
  }, [showToast]);

  useEffect(() => {
    const unlisten = listen("conduit://focus-search", () => {
      setScreen("search");
      setQuery("");
      const input = document.getElementById("search-input") as HTMLInputElement;
      if (input) {
        input.value = "";
        input.focus();
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [setQuery]);

  const activePlugin = results[selectedIndex]?.plugin_id;
  const selectedResult = results[selectedIndex];

  return (
    <div className="conduit-root">
      <div className="drag-handle" data-tauri-drag-region>
        <button
          className={pinned ? "pin-button pinned" : "pin-button"}
          onClick={togglePin}
          title={pinned ? "前面固定を解除" : "前面に固定 (フォーカスを失っても閉じない)"}
          aria-label="前面に固定"
          aria-pressed={pinned}
        >
          📌
        </button>
        <button
          className="close-button"
          onClick={() => {
            try {
              getCurrentWindow().hide().catch(console.error);
            } catch {
              // Not running inside Tauri (browser preview)
            }
          }}
          title="閉じる (Esc)"
          aria-label="閉じる"
        >
          ✕
        </button>
      </div>
      <SearchBar
        value={query}
        onChange={(value) => {
          // Typing anywhere pulls the user back into search
          if (screen !== "search") setScreen("search");
          setQuery(value);
        }}
        trailing={
          <ScreenTabs
            screen={screen}
            onSelect={(next) => {
              setScreen(next);
              if (next === "search") focusSearchInput();
            }}
          />
        }
      />
      {screen === "search" ? (
        <>
          {query.trim() === "" && (
            <KeywordChips
              onPick={(prefix) => {
                setQuery(prefix);
                focusSearchInput();
              }}
            />
          )}
          <ResultList
            results={results}
            selectedIndex={selectedIndex}
            onSelect={setSelectedIndex}
            onExecute={executeResult}
            isLoading={isLoading}
            showSections={query.trim() === ""}
            onContextMenu={openContextMenu}
          />
        </>
      ) : (
        <ManageView
          tab={screen}
          onCycle={cycleScreen}
          onBack={backToSearch}
          onExecute={executeResult}
          keybindings={keybindings}
          config={config}
          onConfigSaved={setConfig}
          onContextMenu={openContextMenu}
        />
      )}
      {isDragOver && (
        <div className="drop-overlay">
          📦 ワークフロー zip をドロップしてインポート
        </div>
      )}
      {screen === "search" && panelOpen && selectedResult && (
        <ActionPanel
          actions={selectedResult.actions}
          selectedIndex={actionIndex}
          onHover={setActionIndex}
          onSelect={(actionId) => {
            executeResult(selectedResult, actionId);
            closePanel();
          }}
        />
      )}
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          actions={contextMenu.result.actions}
          onSelect={(actionId) => executeResult(contextMenu.result, actionId)}
          onClose={() => setContextMenu(null)}
        />
      )}
      {toast && <div className="toast">{toast}</div>}
      <StatusBar pluginName={activePlugin} keybindings={keybindings} />
    </div>
  );
}

export default App;
