import { useEffect, useState, type CSSProperties } from "react";
import { getVersion } from "@tauri-apps/api/app";
import type { Update } from "@tauri-apps/plugin-updater";
import { releaseChannel, saveConfig, summonHotkey } from "../lib/ipc";
import { checkForUpdate, installUpdate, restartApp } from "../lib/updater";
import { useStrings } from "../lib/i18n";
import type { AppConfig } from "../lib/types";
import { AppIcon } from "./AppIcon";
import { MaterialIcon } from "./MaterialIcon";

interface AboutTabProps {
  config: AppConfig | null;
  onConfigSaved: (config: AppConfig) => void;
  onNotify: (message: string) => void;
}

/** Where the update flow has got to. Reported inline rather than through
 *  dialogs — see lib/updater.ts for why. */
type Phase =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "unavailable"; reason: string }
  | { kind: "available"; update: Update }
  | { kind: "installing"; version: string }
  | { kind: "installed"; version: string }
  | { kind: "failed"; error: string };

export function AboutTab({ config, onConfigSaved, onNotify }: AboutTabProps) {
  const s = useStrings();
  const [version, setVersion] = useState<string | null>(null);
  const [channel, setChannel] = useState<string | null>(null);
  const [hotkey, setHotkey] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(null));
    releaseChannel().then(setChannel).catch(() => setChannel(null));
    summonHotkey().then(setHotkey).catch(() => setHotkey(null));
  }, []);

  // Store builds carry no updater. Showing controls that can only fail
  // would be worse than saying plainly where updates come from.
  const selfUpdates = channel === null || channel === "download";

  const autoCheck = config?.auto_update_check !== false;

  const toggleAutoCheck = () => {
    if (!config) return;
    const updated = { ...config, auto_update_check: !autoCheck };
    onConfigSaved(updated);
    saveConfig(updated).catch((err) =>
      onNotify(s("about", "save_failed", { error: String(err) })),
    );
  };

  const runCheck = async () => {
    setPhase({ kind: "checking" });
    const result = await checkForUpdate();
    if (result.status === "available") setPhase({ kind: "available", update: result.update });
    else if (result.status === "current") setPhase({ kind: "current" });
    else setPhase({ kind: "unavailable", reason: result.reason });
  };

  const runInstall = async (update: Update) => {
    setPhase({ kind: "installing", version: update.version });
    try {
      await installUpdate(update);
      setPhase({ kind: "installed", version: update.version });
    } catch (err) {
      setPhase({ kind: "failed", error: String(err) });
    }
  };

  return (
    <div style={cardStyle}>
      <div style={headerStyle}>
        <AppIcon size={40} />
        <div>
          <div style={nameStyle}>Conduit</div>
          <div style={versionStyle}>
            {version ? s("about", "version", { version }) : "…"}
            {channel && channel !== "download" && (
              <span style={{ marginLeft: "6px" }}>
                {s("about", `channel_${channel}`)}
              </span>
            )}
          </div>
        </div>
      </div>

      {hotkey && (
        <p style={statusStyle}>
          {s("about", "summon_hotkey", { hotkey })}
          <span style={hintStyle}>{s("about", "summon_hotkey_hint")}</span>
        </p>
      )}

      {!selfUpdates && (
        <p style={statusStyle}>{s("about", "store_build")}</p>
      )}

      {selfUpdates && (
      <>
      <label style={toggleRowStyle}>
        <input type="checkbox" checked={autoCheck} onChange={toggleAutoCheck} />
        <span>
          <div>{s("about", "auto_check")}</div>
          <div style={hintStyle}>{s("about", "auto_check_hint")}</div>
        </span>
      </label>

      <div style={actionRowStyle}>
        <button
          style={buttonStyle}
          onClick={runCheck}
          disabled={phase.kind === "checking" || phase.kind === "installing"}
        >
          <MaterialIcon name="restart_alt" size={14} />
          {phase.kind === "checking" ? s("about", "checking") : s("about", "check_now")}
        </button>
        {phase.kind === "available" && (
          <button style={primaryButtonStyle} onClick={() => runInstall(phase.update)}>
            {s("about", "install", { version: phase.update.version })}
          </button>
        )}
        {phase.kind === "installed" && (
          <button style={primaryButtonStyle} onClick={() => restartApp().catch((e) => onNotify(String(e)))}>
            {s("about", "restart")}
          </button>
        )}
      </div>

      <StatusLine phase={phase} />
      </>
      )}
    </div>
  );
}

function StatusLine({ phase }: { phase: Phase }) {
  const s = useStrings();
  switch (phase.kind) {
    case "current":
      return <p style={statusStyle}>{s("about", "up_to_date")}</p>;
    case "available":
      return <p style={statusStyle}>{s("about", "available", { version: phase.update.version })}</p>;
    case "installing":
      return <p style={statusStyle}>{s("about", "installing", { version: phase.version })}</p>;
    case "installed":
      return <p style={statusStyle}>{s("about", "installed", { version: phase.version })}</p>;
    case "unavailable":
      // A dev build or a build predating the signing key. Say so plainly
      // instead of dressing it up as a failure.
      return (
        <p style={statusStyle}>
          {s("about", "unavailable")}
          <span style={hintStyle}>{phase.reason}</span>
        </p>
      );
    case "failed":
      return <p style={{ ...statusStyle, color: "var(--text-accent)" }}>{phase.error}</p>;
    default:
      return null;
  }
}

const cardStyle: CSSProperties = {
  background: "var(--bg-secondary)",
  border: "1px solid var(--border-color)",
  borderRadius: "12px",
  padding: "16px",
  display: "flex",
  flexDirection: "column",
  gap: "14px",
};

const headerStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "12px",
};

const nameStyle: CSSProperties = {
  fontSize: "15px",
  fontWeight: 600,
  color: "var(--text-primary)",
};

const versionStyle: CSSProperties = {
  fontSize: "12px",
  color: "var(--text-secondary)",
};

const toggleRowStyle: CSSProperties = {
  display: "flex",
  alignItems: "flex-start",
  gap: "8px",
  fontSize: "12px",
  color: "var(--text-primary)",
  cursor: "pointer",
};

const hintStyle: CSSProperties = {
  display: "block",
  fontSize: "11px",
  color: "var(--text-secondary)",
  marginTop: "2px",
};

const actionRowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "8px",
  flexWrap: "wrap",
};

const buttonStyle: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: "5px",
  background: "var(--bg-hover)",
  border: "1px solid var(--border-color)",
  borderRadius: "999px",
  color: "var(--text-primary)",
  fontSize: "12px",
  padding: "5px 12px",
  cursor: "pointer",
};

const primaryButtonStyle: CSSProperties = {
  ...buttonStyle,
  background: "var(--text-accent)",
  borderColor: "var(--text-accent)",
  color: "#fff",
};

const statusStyle: CSSProperties = {
  margin: 0,
  fontSize: "12px",
  color: "var(--text-secondary)",
};
