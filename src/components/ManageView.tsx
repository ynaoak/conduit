import { useEffect, useRef, useState, type CSSProperties } from "react";
import { browsePlugin, listWorkflows } from "../lib/ipc";
import { matchChord } from "../lib/keys";
import type {
  AppConfig,
  Keybindings,
  SearchResult,
  WorkflowManifest,
} from "../lib/types";
import { KeybindingsTab } from "./KeybindingsTab";
import { PinsTab } from "./PinsTab";
import { ResultItem } from "./ResultItem";

export type ManageTab = "pins" | "workflows" | "launcher" | "keybindings";

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
  borderRadius: "10px",
  padding: "12px 14px",
  marginBottom: "8px",
};

const chipStyle: CSSProperties = {
  background: "var(--bg-primary)",
  border: "1px solid var(--border-color)",
  borderRadius: "999px",
  fontSize: "11px",
  color: "var(--text-primary)",
  padding: "2px 10px",
  whiteSpace: "nowrap",
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

function WorkflowsTab({ keybindings }: { keybindings: Keybindings }) {
  const [workflows, setWorkflows] = useState<WorkflowManifest[] | null>(null);
  const [selected, setSelected] = useState(0);
  const itemRefs = useRef<(HTMLDivElement | null)[]>([]);

  useEffect(() => {
    listWorkflows()
      .then(setWorkflows)
      .catch(() => setWorkflows([]));
  }, []);

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

  if (workflows && workflows.length === 0) {
    return (
      <div style={emptyStyle}>
        <span style={{ fontSize: "28px", opacity: 0.5 }}>📦</span>
        <span>ワークフローはまだありません</span>
        <span style={{ fontSize: "11px", opacity: 0.8 }}>
          manifest.json 入りの zip をこのウィンドウにドロップすると追加できます
        </span>
      </div>
    );
  }

  return (
    <>
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
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "8px", marginBottom: "4px" }}>
            <span style={{ fontSize: "16px" }}>{workflow.icon ?? "🌐"}</span>
            <span style={{ fontSize: "14px", color: "var(--text-primary)", fontWeight: 600 }}>
              {workflow.name}
            </span>
            <span style={{ fontSize: "11px", color: "var(--text-secondary)" }}>
              {workflow.id}
            </span>
          </div>
          {workflow.description && (
            <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
              {workflow.description}
            </div>
          )}
          <div style={{ display: "flex", flexWrap: "wrap", gap: "6px", marginTop: "8px" }}>
            {workflow.apps.map((app) => (
              <span key={`${workflow.id}:${app.name}`} style={chipStyle} title={app.url}>
                {app.icon ? `${app.icon} ` : ""}
                {app.name}
                {app.keyword && (
                  <span style={{ color: "var(--text-accent)" }}> {app.keyword} ▸</span>
                )}
              </span>
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
  const [apps, setApps] = useState<SearchResult[] | null>(null);
  const [selected, setSelected] = useState(0);
  const itemRefs = useRef<(HTMLDivElement | null)[]>([]);

  useEffect(() => {
    browsePlugin("conduit.app-launcher")
      .then(setApps)
      .catch(() => setApps([]));
  }, []);

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
        <span style={{ fontSize: "28px", opacity: 0.5 }}>🚀</span>
        <span>登録済みアプリが見つかりません</span>
        <span style={{ fontSize: "11px", opacity: 0.8 }}>
          起動直後はスキャン中の場合があります。少し待って開き直してください
        </span>
      </div>
    );
  }

  return (
    <>
      {apps && (
        <div style={{ fontSize: "11px", color: "var(--text-secondary)", padding: "0 8px 6px" }}>
          {apps.length} 件 ・ 起動回数順 ・ ↑↓←→ 移動 ・ パスはホバー
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
        {tab === "workflows" && <WorkflowsTab keybindings={keybindings} />}
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
      </div>
    </div>
  );
}
