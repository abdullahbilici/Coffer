import { useEffect, useState } from "react";
import { Copy as CopyIcon, Fingerprint, RefreshCw } from "lucide-react";
import type { Copy, Run } from "../../app/actions";
import { api, type BackupSettings } from "../../lib/api";
import { chooseFolder } from "../../lib/dialogs";
import { displayPath } from "../../lib/paths";
import { Button } from "../ui/Button";
import { IconButton } from "../ui/IconButton";
import { Input } from "../ui/Input";
import styles from "./Settings.module.css";

/** Mirrors `backup::MIN_PASSPHRASE_LEN`. */
const MIN_LENGTH = 12;

type Props = {
  /** The current folder when changing the passphrase; `null` during setup. */
  dir: string | null;
  run: Run;
  onCopy: Copy;
  onDone: (backup: BackupSettings) => void;
  onCancel?: () => void;
};

/**
 * Sets up backups or changes the passphrase: a suggested word passphrase (editable), typed once
 * more to confirm, and Touch ID. Setup also picks the folder.
 */
export function PassphraseForm({ dir: currentDir, run, onCopy, onDone, onCancel }: Props) {
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [dir, setDir] = useState(currentDir);
  const [busy, setBusy] = useState(false);
  const setup = currentDir === null;

  const suggest = () => run(async () => setPassphrase(await api.suggestPassphrase()));

  useEffect(() => {
    void suggest();
    if (setup) void run(async () => setDir(await api.defaultBackupDir()));
    // Once, when the form opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const choose = () =>
    run(async () => {
      const chosen = await chooseFolder(dir);
      if (chosen) setDir(chosen);
    });

  const ready =
    passphrase.trim().length >= MIN_LENGTH && confirm === passphrase && dir !== null && !busy;

  const submit = () =>
    run(async () => {
      if (!ready) return;
      setBusy(true);
      try {
        onDone((await api.setBackupPassphrase(passphrase, dir)).backup);
      } finally {
        setBusy(false);
      }
    });

  return (
    <form
      className={styles.form}
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <label className={styles.field}>
        <span className={styles.fieldLabel}>Passphrase</span>
        <span className={styles.inline}>
          <Input
            className={styles.mono}
            value={passphrase}
            onChange={(e) => setPassphrase(e.target.value)}
          />
          <IconButton icon={RefreshCw} label="Suggest another" onClick={suggest} />
          <IconButton
            icon={CopyIcon}
            label="Copy"
            onClick={() => void run(() => onCopy(passphrase, true))}
          />
        </span>
      </label>
      <label className={styles.field}>
        <span className={styles.fieldLabel}>Confirm</span>
        <Input
          className={styles.mono}
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          onPaste={(e) => e.preventDefault()}
        />
      </label>
      {setup && (
        <div className={styles.field}>
          <span className={styles.fieldLabel}>Folder</span>
          <span className={styles.inline}>
            <span className={styles.path} title={dir ?? undefined}>
              {dir && displayPath(dir)}
            </span>
            <Button size="sm" onClick={choose}>
              Choose…
            </Button>
          </span>
        </div>
      )}
      <div className={styles.actions}>
        {onCancel && (
          <Button size="sm" variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
        )}
        <Button type="submit" size="sm" variant="primary" icon={Fingerprint} disabled={!ready}>
          {setup ? "Set passphrase" : "Change passphrase"}
        </Button>
      </div>
    </form>
  );
}
