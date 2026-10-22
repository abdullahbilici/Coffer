import { useState } from "react";
import type { Copy, Run } from "../../app/actions";
import { useNow } from "../../hooks/useNow";
import type { Session } from "../../hooks/useSession";
import { api, type BackupSettings } from "../../lib/api";
import { chooseBackupFile, chooseFolder } from "../../lib/dialogs";
import { displayPath } from "../../lib/paths";
import { formatAgo } from "../../lib/time";
import { Button } from "../ui/Button";
import { Switch } from "../ui/Switch";
import { PassphraseForm } from "./PassphraseForm";
import { RestoreForm } from "./RestoreForm";
import { SettingRow } from "./SettingRow";
import styles from "./Settings.module.css";

type Props = {
  backup: BackupSettings;
  onChange: (backup: BackupSettings) => void;
  session: Session;
  run: Run;
  onCopy: Copy;
  onRestored: () => Promise<void>;
};

export function BackupSection({ backup, onChange, session, run, onCopy, onRestored }: Props) {
  const [changing, setChanging] = useState(false);
  const [restorePath, setRestorePath] = useState<string | null>(null);

  const change = (action: () => Promise<{ backup: BackupSettings }>) =>
    run(async () => onChange((await session.withUnlock(action)).backup));

  const chooseDir = () =>
    run(async () => {
      const dir = await chooseFolder(backup.dir);
      if (dir) await change(() => api.setBackupDir(dir));
    });

  const chooseRestore = () =>
    run(async () => {
      const path = await chooseBackupFile();
      if (path) setRestorePath(path);
    });

  const restore = (
    <>
      <SettingRow label="Restore">
        <Button size="sm" onClick={chooseRestore}>
          Choose file…
        </Button>
      </SettingRow>
      {restorePath && (
        <RestoreForm
          key={restorePath}
          path={restorePath}
          confirm
          run={run}
          onRestored={async () => {
            setRestorePath(null);
            await onRestored();
          }}
          onCancel={() => setRestorePath(null)}
        />
      )}
    </>
  );

  if (!backup.configured) {
    return (
      <>
        <PassphraseForm dir={null} run={run} onCopy={onCopy} onDone={onChange} />
        {restore}
      </>
    );
  }

  return (
    <>
      <SettingRow label="Last backup">
        <LastBackup backup={backup} />
        <Button size="sm" onClick={() => void change(api.backUpNow)}>
          Back up now
        </Button>
      </SettingRow>
      <SettingRow label="Automatic backups">
        <Switch
          label="Automatic backups"
          labelHidden
          checked={backup.auto}
          onChange={(on) => void change(() => api.setBackupAuto(on))}
        />
      </SettingRow>
      <SettingRow label="Folder">
        <span className={styles.path} title={backup.dir ?? undefined}>
          {backup.dir && displayPath(backup.dir)}
        </span>
        <Button size="sm" onClick={chooseDir}>
          Choose…
        </Button>
      </SettingRow>
      <SettingRow label="Passphrase">
        <Button size="sm" disabled={changing} onClick={() => setChanging(true)}>
          Change…
        </Button>
      </SettingRow>
      {changing && (
        <PassphraseForm
          dir={backup.dir}
          run={run}
          onCopy={onCopy}
          onDone={(next) => {
            setChanging(false);
            onChange(next);
          }}
          onCancel={() => setChanging(false)}
        />
      )}
      {restore}
    </>
  );
}

/** Ticks so "5 min ago" stays true while Settings is open. */
function LastBackup({ backup }: { backup: BackupSettings }) {
  const now = useNow(30_000);
  if (backup.lastError) {
    return (
      <span className={styles.error} title={backup.lastError}>
        {backup.lastError}
      </span>
    );
  }
  return (
    <span className={styles.value}>
      {backup.lastAt === null ? "Never" : formatAgo(backup.lastAt, now)}
    </span>
  );
}
