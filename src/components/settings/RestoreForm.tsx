import { useState } from "react";
import { ArchiveRestore } from "lucide-react";
import type { Run } from "../../app/actions";
import { api } from "../../lib/api";
import { fileName } from "../../lib/paths";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import styles from "./Settings.module.css";

/** Second-click label for replacing the vault with a backup. */
export const CONFIRM_RESTORE = "Confirm restore";

type Props = {
  /** A chosen `.coffer` file. */
  path: string;
  /** Ask for a second click first (the vault has something to lose). */
  confirm: boolean;
  run: Run;
  onRestored: () => Promise<void>;
  onCancel: () => void;
};

/** The passphrase for a chosen backup and the button that replaces the vault with it. */
export function RestoreForm({ path, confirm, run, onRestored, onCancel }: Props) {
  const [passphrase, setPassphrase] = useState("");
  const [armed, setArmed] = useState(false);
  const [busy, setBusy] = useState(false);

  const submit = () =>
    run(async () => {
      if (!passphrase || busy) return;
      if (confirm && !armed) {
        setArmed(true);
        return;
      }
      setBusy(true);
      try {
        await api.restoreBackup(path, passphrase);
        await onRestored();
      } finally {
        setBusy(false);
        setArmed(false);
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
        <span className={styles.fieldLabel}>{fileName(path)}</span>
        <Input
          type="password"
          placeholder="Passphrase"
          autoFocus
          value={passphrase}
          onChange={(e) => {
            setPassphrase(e.target.value);
            setArmed(false);
          }}
        />
      </label>
      <div className={styles.actions}>
        <Button size="sm" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          type="submit"
          size="sm"
          variant={armed ? "danger" : "primary"}
          icon={ArchiveRestore}
          disabled={!passphrase || busy}
        >
          {armed ? CONFIRM_RESTORE : "Restore"}
        </Button>
      </div>
    </form>
  );
}
