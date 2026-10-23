import { useEffect, useState } from "react";
import type { Copy, Run } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { api } from "../../lib/api";
import { Switch } from "../ui/Switch";
import { SettingRow } from "./SettingRow";
import styles from "./Settings.module.css";

type Props = {
  mcpAccess: boolean;
  onChange: (mcpAccess: boolean) => void;
  session: Session;
  run: Run;
  onCopy: Copy;
};

/** The MCP switch and, while it's on, the command that registers Coffer with Claude Code. */
export function IntegrationsSection({ mcpAccess, onChange, session, run, onCopy }: Props) {
  const [command, setCommand] = useState<string | null>(null);

  useEffect(() => {
    void run(async () => setCommand(await api.mcpCommand()));
  }, [run]);

  const setAccess = (on: boolean) =>
    run(async () => {
      await session.withUnlock(() => api.setMcpAccess(on));
      onChange(on);
    });

  return (
    <>
      <SettingRow label="Allow MCP access">
        <Switch
          label="Allow MCP access"
          labelHidden
          checked={mcpAccess}
          onChange={(on) => void setAccess(on)}
        />
      </SettingRow>
      {mcpAccess && command && (
        <div className={styles.form}>
          <div className={styles.field}>
            <span className={styles.fieldLabel}>Claude Code</span>
            <button
              type="button"
              className={styles.command}
              onClick={() => void run(() => onCopy(command))}
            >
              {command}
            </button>
          </div>
        </div>
      )}
    </>
  );
}
