import { useState } from "react";
import { ArchiveRestore, Fingerprint } from "lucide-react";
import type { Run } from "../../app/actions";
import logoUrl from "../../assets/logo.svg";
import { chooseBackupFile } from "../../lib/dialogs";
import { RestoreForm } from "../settings/RestoreForm";
import { Button } from "../ui/Button";
import styles from "./LockScreen.module.css";

type Props = {
  onUnlock: () => void;
  /** The vault is empty, so a backup can replace it without losing anything. */
  canRestore: boolean;
  run: Run;
  onRestored: () => Promise<void>;
};

export function LockScreen({ onUnlock, canRestore, run, onRestored }: Props) {
  const [restorePath, setRestorePath] = useState<string | null>(null);

  const chooseRestore = () =>
    run(async () => {
      const path = await chooseBackupFile();
      if (path) setRestorePath(path);
    });

  return (
    <main className={styles.root} data-tauri-drag-region>
      <img src={logoUrl} alt="" className={styles.logo} draggable={false} />
      <h1 className={styles.title}>Coffer</h1>
      {restorePath ? (
        <div className={styles.restore}>
          <RestoreForm
            path={restorePath}
            confirm={false}
            run={run}
            onRestored={onRestored}
            onCancel={() => setRestorePath(null)}
          />
        </div>
      ) : (
        <div className={styles.actions}>
          <Button variant="primary" icon={Fingerprint} onClick={onUnlock}>
            Unlock with Touch ID
          </Button>
          {canRestore && (
            <Button variant="ghost" icon={ArchiveRestore} onClick={chooseRestore}>
              Restore from backup
            </Button>
          )}
        </div>
      )}
    </main>
  );
}
