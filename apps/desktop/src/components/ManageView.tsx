import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import {
  browsePlugin,
  deleteWorkflow,
  exportWorkflow,
  executeAction,
  autoHideWindow,
  importWorkflow,
  listWorkflows,
  setWorkflowEnabled,
} from "../lib/ipc";
import { useLanguage, useStrings } from "../lib/i18n";
import { matchChord } from "../lib/keys";
import { MaterialIcon } from "./MaterialIcon";
import type {
  AppConfig,
  AppListing,
  Keybindings,
  SearchResult,
  WorkflowListing,
} from "../lib/types";
import { AboutTab } from "./AboutTab";
import { KeybindingsTab } from "./KeybindingsTab";
import { PinsTab } from "./PinsTab";
import { ResultItem } from "./ResultItem";

export type ManageTab =
  | "pins"
  | "workflows"
  | "launcher"
  | "keybindings"
  | "about";

interface ManageViewProps {
  tab: ManageTab;
  /** Cycle across all screens (search included); +1 forward, -1 backward */
  onCycle: (direction: 1 | -1) => void;
  onBack: () => void;
  onExecute: (result: SearchResult, actionId?: string) => void;
  keybindings: Keybindings;
  config: AppConfig | null;
  onConfigSaved: (config: AppConfig) => void;
  onContextMenu?: (result: SearchResult, x: number, y: number) => void;
  /** Surface a toast in the shell (import / delete / launch feedback) */
  onNotify: (message: string) => void;
}

const rootStyle: CSSProperties = {
  flex: 1,
  display: "flex",
  flexDirection: "column",
  minHeight: 0,
};

const bodyStyle: CSSProperties = {
  flex: 1,
  overflowY: "auto",
  padding: "4px 12px 12px",
  minHeight: 0,
};

const cardStyle: CSSProperties = {
  background: "var(--bg-secondary)",
  border: "1px solid var(--border-color)",
  borderRadius: "12px",
  padding: "12px 14px",
  marginBottom: "8px",
};

/** App chips are launch buttons (M3 chip shape) */
const chipStyle: CSSProperties = {
  background: "var(--bg-primary)",
  border: "1px solid var(--border-color)",
  borderRadius: "8px",
  fontSize: "11px",
  color: "var(--text-primary)",
  padding: "3px 12px",
  whiteSpace: "nowrap",
  cursor: "pointer",
  transition: "border-color var(--transition-fast)",
};

const actionButtonStyle: CSSProperties = {
  background: "transparent",
  border: "1px solid var(--border-color)",
  borderRadius: "999px",
  fontSize: "11px",
  color: "var(--text-secondary)",
  padding: "2px 10px",
  cursor: "pointer",
  flexShrink: 0,
};

/** Columns in the launcher grid. Keyboard navigation derives its row step
 *  from this, so the two never drift apart. */
const LAUNCHER_COLUMNS = 2;

const launcherGridStyle: CSSProperties = {
  display: "grid",
  // minmax(0, 1fr) rather than 1fr: without it a long app name refuses to
  // shrink below its content width and blows the column out
  gridTemplateColumns: `repeat(${LAUNCHER_COLUMNS}, minmax(0, 1fr))`,
  gap: "2px 6px",
};

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

function WorkflowsTab({
  keybindings,
  onNotify,
}: {
  keybindings: Keybindings;
  onNotify: (message: string) => void;
}) {
  const s = useStrings();
  const lang = useLanguage();
  const [workflows, setWorkflows] = useState<WorkflowListing[] | null>(null);
  const [selected, setSelected] = useState(0);
  const itemRefs = useRef<(HTMLDivElement | null)[]>([]);

  const refresh = useCallback(() => {
    listWorkflows()
      .then(setWorkflows)
      .catch(() => setWorkflows([]));
  }, []);

  // Re-ask on a language change too: the built-in package's name, its
  // description and its app buttons are localized in Rust and arrive as
  // finished strings, so nothing a re-render does can translate them.
  useEffect(refresh, [refresh, lang]);

  // Up/down moves the card selection
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!workflows || workflows.length === 0) return;
      if (matchChord(e, keybindings.move_down)) {
        e.preventDefault();
        setSelected((i) => Math.min(i + 1, workflows.length - 1));
      } else if (matchChord(e, keybindings.move_up)) {
        e.preventDefault();
        setSelected((i) => Math.max(i - 1, 0));
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [workflows, keybindings]);

  useEffect(() => {
    itemRefs.current[selected]?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  const importFromDialog = useCallback(() => {
    openFileDialog({
      multiple: false,
      filters: [{ name: "Workflow zip", extensions: ["zip"] }],
    })
      .then((path) => {
        if (typeof path !== "string") return; // cancelled
        return importWorkflow(path).then((name) => {
          onNotify(s("app", "toast_imported", { name }));
          refresh();
        });
      })
      .catch((err) => onNotify(s("app", "toast_import_failed", { error: String(err) })));
  }, [onNotify, refresh]);

  const launch = useCallback(
    (app: AppListing) => {
      executeAction("conduit.workflows", app.result_id, "open")
        .then(() => autoHideWindow().catch(console.error))
        .catch((err) => onNotify(s("manage", "workflows_launch_failed", { error: String(err) })));
    },
    [onNotify],
  );

  const toggleEnabled = useCallback(
    (workflow: WorkflowListing) => {
      setWorkflowEnabled(workflow.id, workflow.disabled)
        .then(() => {
          onNotify(
            workflow.disabled
              ? s("manage", "workflows_enabled_toast", { name: workflow.name })
              : s("manage", "workflows_disabled_toast", { name: workflow.name }),
          );
          refresh();
        })
        .catch((err) => onNotify(s("manage", "workflows_change_failed", { error: String(err) })));
    },
    [onNotify, refresh],
  );

  // Exporting the built-in package is the documented way to see a working
  // package: unzip it and the manifest layout is right there.
  const exportToZip = useCallback(
    (workflow: WorkflowListing) => {
      saveFileDialog({
        defaultPath: `${workflow.id}.zip`,
        filters: [{ name: "Workflow zip", extensions: ["zip"] }],
      })
        .then((dest) => {
          if (typeof dest !== "string") return; // cancelled
          return exportWorkflow(workflow.id, dest).then((path) =>
            onNotify(s("manage", "workflows_exported_toast", { path })),
          );
        })
        .catch((err) =>
          onNotify(s("manage", "workflows_export_failed", { error: String(err) })),
        );
    },
    [onNotify],
  );

  const remove = useCallback(
    (workflow: WorkflowListing) => {
      deleteWorkflow(workflow.id)
        .then(() => {
          onNotify(s("manage", "workflows_deleted_toast", { name: workflow.name }));
          refresh();
        })
        .catch((err) => onNotify(s("manage", "workflows_delete_failed", { error: String(err) })));
    },
    [onNotify, refresh],
  );

  const importRow = (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: "8px",
        padding: "0 4px 8px",
      }}
    >
      <button style={actionButtonStyle} onClick={importFromDialog}>
        {s("manage", "workflows_import")}
      </button>
      <span style={{ fontSize: "11px", color: "var(--text-secondary)" }}>
        {s("manage", "workflows_drop_hint")}
      </span>
    </div>
  );

  if (workflows && workflows.length === 0) {
    return (
      <>
        {importRow}
        <div style={emptyStyle}>
          <MaterialIcon name="deployed_code" size={36} color="var(--text-secondary)" style={{ opacity: 0.6 }} />
          <span>{s("manage", "workflows_empty")}</span>
        </div>
      </>
    );
  }

  return (
    <>
      {importRow}
      {workflows?.map((workflow, index) => (
        <div
          key={workflow.id}
          ref={(el) => {
            itemRefs.current[index] = el;
          }}
          onMouseEnter={() => setSelected(index)}
          style={{
            ...cardStyle,
            ...(index === selected
              ? { borderColor: "var(--text-accent)" }
              : {}),
            ...(workflow.disabled ? { opacity: 0.55 } : {}),
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "8px", marginBottom: "4px" }}>
            <span style={{ fontSize: "16px" }}>{workflow.icon ?? "🌐"}</span>
            <span style={{ fontSize: "14px", color: "var(--text-primary)", fontWeight: 600 }}>
              {workflow.name}
            </span>
            {workflow.builtin && (
              <span
                style={{
                  fontSize: "10px",
                  color: "var(--text-accent)",
                  border: "1px solid var(--text-accent)",
                  borderRadius: "999px",
                  padding: "0 8px",
                }}
              >
                {s("manage", "workflows_builtin")}
              </span>
            )}
            <span style={{ fontSize: "11px", color: "var(--text-secondary)", flex: 1 }}>
              {workflow.id}
            </span>
            <button style={actionButtonStyle} onClick={() => exportToZip(workflow)}>
              {s("manage", "workflows_export")}
            </button>
            <button style={actionButtonStyle} onClick={() => toggleEnabled(workflow)}>
              {s("manage", workflow.disabled ? "workflows_enable" : "workflows_disable")}
            </button>
            {!workflow.builtin && (
              <button
                style={{ ...actionButtonStyle, color: "var(--text-accent)" }}
                onClick={() => remove(workflow)}
              >
                {s("manage", "workflows_delete")}
              </button>
            )}
          </div>
          {workflow.description && (
            <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
              {workflow.description}
            </div>
          )}
          <div style={{ display: "flex", flexWrap: "wrap", gap: "6px", marginTop: "8px" }}>
            {workflow.apps.map((app) => (
              <button
                key={`${workflow.id}:${app.name}`}
                style={{
                  ...chipStyle,
                  ...(workflow.disabled ? { cursor: "default" } : {}),
                }}
                title={
                  workflow.disabled
                    ? s("manage", "workflows_disabled_hint")
                    : s("manage", "workflows_launch_hint", {
                        target: app.url ?? app.html ?? "",
                      })
                }
                disabled={workflow.disabled}
                onClick={() => launch(app)}
                onMouseEnter={(e) => {
                  if (!workflow.disabled)
                    (e.currentTarget as HTMLButtonElement).style.borderColor =
                      "var(--text-accent)";
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLButtonElement).style.borderColor =
                    "var(--border-color)";
                }}
              >
                {app.icon && !/^[a-z0-9_]+$/.test(app.icon) ? `${app.icon} ` : ""}
                {app.name}
                {app.keyword && (
                  <span style={{ color: "var(--text-accent)" }}> {app.keyword} ▸</span>
                )}
              </button>
            ))}
          </div>
        </div>
      ))}
    </>
  );
}

function LauncherTab({
  onExecute,
  keybindings,
  onContextMenu,
}: {
  onExecute: ManageViewProps["onExecute"];
  keybindings: Keybindings;
  onContextMenu?: ManageViewProps["onContextMenu"];
}) {
  const s = useStrings();
  const lang = useLanguage();
  const [apps, setApps] = useState<SearchResult[] | null>(null);
  const [selected, setSelected] = useState(0);
  const itemRefs = useRef<(HTMLDivElement | null)[]>([]);

  // Same as the workflows tab: the subtitles ("Store app") and the action
  // names on each entry are worded by Rust, so a language switch re-asks
  useEffect(() => {
    browsePlugin("conduit.app-launcher")
      .then(setApps)
      .catch(() => setApps([]));
  }, [lang]);

  // Up/down moves a whole row, left/right moves a column, execute launches.
  // ManageView yields the horizontal arrows to this grid (see LAUNCHER_GRID
  // note there) — Tab still cycles tabs.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!apps || apps.length === 0) return;
      const last = apps.length - 1;
      if (matchChord(e, keybindings.move_down)) {
        e.preventDefault();
        setSelected((i) => Math.min(i + LAUNCHER_COLUMNS, last));
      } else if (matchChord(e, keybindings.move_up)) {
        e.preventDefault();
        // Staying put on the top row beats snapping to a different column
        setSelected((i) => (i - LAUNCHER_COLUMNS < 0 ? i : i - LAUNCHER_COLUMNS));
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        setSelected((i) => Math.min(i + 1, last));
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        setSelected((i) => Math.max(i - 1, 0));
      } else if (matchChord(e, keybindings.execute)) {
        e.preventDefault();
        const app = apps[selected];
        if (app) onExecute(app);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [apps, selected, keybindings, onExecute]);

  useEffect(() => {
    itemRefs.current[selected]?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  if (apps && apps.length === 0) {
    return (
      <div style={emptyStyle}>
        <MaterialIcon name="rocket_launch" size={36} color="var(--text-secondary)" style={{ opacity: 0.6 }} />
        <span>{s("manage", "launcher_empty")}</span>
        <span style={{ fontSize: "11px", opacity: 0.8 }}>
          {s("manage", "launcher_empty_hint")}
        </span>
      </div>
    );
  }

  return (
    <>
      {apps && (
        <div style={{ fontSize: "11px", color: "var(--text-secondary)", padding: "0 8px 6px" }}>
          {s("manage", "launcher_summary", { count: apps.length })}
        </div>
      )}
      <div style={launcherGridStyle}>
        {apps?.map((app, index) => (
          <div
            key={app.id}
            ref={(el) => {
              itemRefs.current[index] = el;
            }}
            style={{ minWidth: 0 }}
          >
            <ResultItem
              compact
              result={app}
              isSelected={index === selected}
              onClick={() => {
                setSelected(index);
                onExecute(app);
              }}
              onContextMenu={(r, x, y) => {
                setSelected(index);
                onContextMenu?.(r, x, y);
              }}
            />
          </div>
        ))}
      </div>
    </>
  );
}

export function ManageView({
  tab,
  onCycle,
  onBack,
  onExecute,
  keybindings,
  config,
  onConfigSaved,
  onContextMenu,
  onNotify,
}: ManageViewProps) {
  const [recording, setRecording] = useState(false);

  // Keyboard: Esc / manage_view chord goes back, tab_next / tab_prev cycle
  // through every screen (wrapping into search). Paused while the
  // keybindings tab is recording a key.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (recording) return;

      if (e.key === "Escape" || matchChord(e, keybindings.manage_view)) {
        e.preventDefault();
        e.stopPropagation();
        onBack();
        return;
      }
      // The launcher is a grid, so it claims Left/Right for column movement.
      // Tab / Ctrl+Tab still cycle tabs there.
      if (tab === "launcher" && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        return;
      }

      if (matchChord(e, keybindings.tab_next)) {
        e.preventDefault();
        e.stopPropagation();
        onCycle(1);
        return;
      }
      if (matchChord(e, keybindings.tab_prev)) {
        e.preventDefault();
        e.stopPropagation();
        onCycle(-1);
      }
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [onBack, onCycle, keybindings, recording, tab]);

  // Tabs live in the persistent SearchBar row (ScreenTabs); this view only
  // renders the active tab's content
  return (
    <div style={rootStyle}>
      <div style={bodyStyle}>
        {tab === "pins" && (
          <PinsTab
            onExecute={onExecute}
            keybindings={keybindings}
            onContextMenu={onContextMenu}
          />
        )}
        {tab === "workflows" && (
          <WorkflowsTab keybindings={keybindings} onNotify={onNotify} />
        )}
        {tab === "launcher" && (
          <LauncherTab
            onExecute={onExecute}
            keybindings={keybindings}
            onContextMenu={onContextMenu}
          />
        )}
        {tab === "keybindings" && (
          <KeybindingsTab
            config={config}
            onConfigSaved={onConfigSaved}
            onRecordingChange={setRecording}
            keybindings={keybindings}
          />
        )}
        {tab === "about" && (
          <AboutTab
            config={config}
            onConfigSaved={onConfigSaved}
            onNotify={onNotify}
          />
        )}
      </div>
    </div>
  );
}
