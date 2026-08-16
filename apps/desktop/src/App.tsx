import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { SearchBar } from "./components/SearchBar";
import { KeywordChips } from "./components/KeywordChips";
import { ResultList } from "./components/ResultList";
import { StatusBar } from "./components/StatusBar";
import { ActionPanel } from "./components/ActionPanel";
import { ContextMenu } from "./components/ContextMenu";
import { ManageView } from "./components/ManageView";
import { MaterialIcon } from "./components/MaterialIcon";
import { ScreenTabs } from "./components/ScreenTabs";
import { type ThemeMode } from "./components/StatusBar";
import { LanguageContext, makeStrings, resolveLanguage } from "./lib/i18n";
import { checkForUpdate } from "./lib/updater";
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
  autoHideWindow,
  saveConfig,
  hideWindow,
  resolvedLanguage,
  releaseChannel,
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
  "about",
];

function focusSearchInput() {
  setTimeout(() => {
    (document.getElementById("search-input") as HTMLInputElement)?.focus();
  }, 0);
}

/** Actions whose result lives on the clipboard — keep the window open and
 *  confirm with a toast instead of silently hiding */
const COPY_ACTIONS = new Set(["copy", "copy-path", "copy-url", "copy-pid"]);

/** theme.* colour overrides, which a hand-picked mode clears. Deliberately
 *  not border_radius: that is a shape preference, unrelated to light/dark. */
const THEME_COLOR_SLOTS = [
  "bg_primary",
  "bg_secondary",
  "bg_hover",
  "bg_selected",
  "text_primary",
  "text_secondary",
  "text_accent",
  "border_color",
] as const satisfies readonly (keyof AppConfig["theme"])[];

/** Synthetic action ids handled by the shell, not by any plugin */
const PIN_ACTION = "__pin";
const UNPIN_ACTION = "__unpin";

function App() {
  // Resolved here rather than through the hook: App both uses the strings
  // (toasts, tooltips) and provides them to everything below.
  const [config, setConfig] = useState<AppConfig | null>(null);
  // Follow the backend rather than resolving "system" here: plugin results
  // are localized in Rust off the OS locale, and navigator.languages can
  // name a different one — mirroring it keeps one window in one language.
  const [backendLang, setBackendLang] = useState<string | null>(null);
  // Asked on mount and again once a language change has been *saved*:
  // save_config is what applies the setting in Rust, so reading before it
  // resolves can hand back the language the user just left.
  const syncBackendLanguage = useCallback(() => {
    resolvedLanguage().then(setBackendLang).catch(() => setBackendLang(null));
  }, []);
  useEffect(syncBackendLanguage, [syncBackendLanguage]);
  const lang = resolveLanguage(backendLang ?? config?.language);
  const s = useMemo(() => makeStrings(lang), [lang]);
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
  // Results carry Rust-localized text, so the query re-runs on a language
  // change (see useSearch)
  const { query, setQuery, results: rawResults, isLoading, refresh } = useSearch(lang);

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
          ? { id: UNPIN_ACTION, title: s("app", "unpin_action"), shortcut: null }
          : { id: PIN_ACTION, title: s("app", "pin_action"), shortcut: null },
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
                ? s("app", "toast_pinned")
                : s("app", "toast_unpinned"),
            );
          })
          .catch((err) => showToast(s("app", "toast_failed", { error: String(err) })));
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
            showToast(s("app", "toast_killed", { title: result.title }));
            refresh();
          })
          .catch((err) => showToast(s("app", "toast_kill_failed", { error: String(err) })));
        return;
      }

      // Copy actions: stay open and confirm — hiding silently gives no
      // feedback that the clipboard now holds the value
      if (action && COPY_ACTIONS.has(action)) {
        executeAction(result.plugin_id, result.id, action)
          .then(() => showToast(s("app", "toast_copied")))
          .catch((err) => showToast(s("app", "toast_copy_failed", { error: String(err) })));
        return;
      }

      // Hide only after the action succeeded: a silent failure that also
      // closes the launcher looks like "nothing happened" and is
      // undiagnosable from the UI
      executeAction(result.plugin_id, result.id, action)
        .then(() => autoHideWindow().catch(console.error))
        .catch((err) => showToast(s("app", "toast_execute_failed", { error: String(err) })));
    },
    [refresh, showToast, pinnedIds],
  );

  const keybindings = config?.keybindings
    ? { ...DEFAULT_KEYBINDINGS, ...config.keybindings }
    : DEFAULT_KEYBINDINGS;

  const themeMode = (config?.theme.mode ?? "system") as ThemeMode;
  const selectTheme = useCallback(
    (mode: ThemeMode) => {
      if (!config) return;
      // Per-slot colors from config.json are applied as inline styles on
      // the root, which beat the [data-theme] palettes in the stylesheet no
      // matter what the mode says. Any config written before theme.mode
      // existed carries a full palette, so for those users the mode button
      // would change the setting and nothing else. Picking a theme by hand
      // is an unambiguous request for that theme, so the colors give way.
      const theme: AppConfig["theme"] = { ...config.theme, mode };
      const cleared = THEME_COLOR_SLOTS.filter((slot) => theme[slot]);
      for (const slot of cleared) theme[slot] = null;

      const updated = { ...config, theme };
      // Apply first: useTheme repaints (and re-reports) without waiting on disk
      setConfig(updated);
      if (cleared.length > 0) showToast(s("app", "toast_theme_colors_cleared"));
      saveConfig(updated).catch((err) =>
        showToast(s("app", "toast_theme_save_failed", { error: String(err) })),
      );
    },
    [config, showToast, s],
  );

  const languageMode = config?.language ?? "system";
  const selectLanguage = useCallback(
    (language: string) => {
      if (!config) return;
      const updated = { ...config, language };
      setConfig(updated);
      saveConfig(updated)
        // Only now is the backend rendering in the new language; `lang`
        // follows it, and everything holding backend-worded text re-asks.
        .then(syncBackendLanguage)
        .catch((err) =>
          showToast(s("app", "toast_language_save_failed", { error: String(err) })),
        );
    },
    [config, showToast, s, syncBackendLanguage],
  );

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

  // Silent update check shortly after launch. Only ever raises a toast —
  // the actual install lives in the About tab, because a dialog here would
  // take focus and the launcher hides itself when it loses focus.
  const updateCheckedRef = useRef(false);
  useEffect(() => {
    if (!config || updateCheckedRef.current) return;
    updateCheckedRef.current = true;
    if (config.auto_update_check === false) return;
    const timer = setTimeout(() => {
      // Store builds ship without the updater; asking would only produce the
      // "unavailable" path, so don't ask.
      releaseChannel().then((ch) => {
        if (ch !== "download") return;
        return checkForUpdate().then((result) => {
          if (result.status !== "available") return;
          showToast(s("about", "toast_available", { version: result.update.version }));
        });
      });
    }, 3000);
    return () => clearTimeout(timer);
  }, [config, showToast, s]);

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
          showToast(s("app", "drop_not_zip"));
          return;
        }
        for (const path of zips) {
          try {
            const name = await importWorkflow(path);
            showToast(s("app", "toast_imported", { name }));
          } catch (err) {
            showToast(s("app", "toast_import_failed", { error: String(err) }));
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
    <LanguageContext.Provider value={{ lang, s }}>
    <div className="conduit-root">
      <div className="drag-handle" data-tauri-drag-region>
        <button
          className={pinned ? "pin-button pinned" : "pin-button"}
          onClick={togglePin}
          title={pinned ? s("app", "pin_off") : s("app", "pin_on")}
          aria-label={s("app", "pin_on")}
          aria-pressed={pinned}
        >
          <MaterialIcon name={pinned ? "push_pin_fill" : "push_pin"} size={16} />
        </button>
        <button
          className="close-button"
          onClick={() => {
            hideWindow().catch(console.error);
          }}
          title={s("app", "close")}
          aria-label={s("app", "close")}
        >
          <MaterialIcon name="close" size={16} />
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
          onNotify={showToast}
        />
      )}
      {isDragOver && (
        <div className="drop-overlay">
          📦 {s("app", "drop_overlay")}
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
      <StatusBar
        pluginName={activePlugin}
        keybindings={keybindings}
        themeMode={themeMode}
        onSelectTheme={selectTheme}
        languageMode={languageMode}
        onSelectLanguage={selectLanguage}
      />
    </div>
    </LanguageContext.Provider>
  );
}

export default App;
