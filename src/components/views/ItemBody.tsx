import { useEffect, useState } from "react";
import { Copy, Eye, EyeOff, LockKeyhole } from "lucide-react";
import type { Run } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { api, type Item } from "../../lib/api";
import {
  parseBody,
  parseCopyable,
  parseTemplate,
  secretValues,
  toPlainText,
} from "../../lib/markup";
import { Button } from "../ui/Button";
import { InlineBody } from "./InlineBody";
import styles from "./ItemBody.module.css";

type Props = {
  /** Re-mount (key on `updatedAt`) to reset revealed state after the item changes. */
  item: Item;
  session: Session;
  run: Run;
  /** Writes to the clipboard and confirms with a toast. */
  onCopy: (text: string) => Promise<void>;
};

/** Secret index used for the whole body of a secure item. */
const WHOLE_BODY = 0;

/**
 * An item's body with Copy all, chips and secrets. Used by the item page and search previews.
 * Secrets open masked; decrypted text lives only as long as this component.
 */
export function ItemBody({ item, session, run, onCopy }: Props) {
  const [plaintext, setPlaintext] = useState<string | null>(null);
  const [shown, setShown] = useState<ReadonlySet<number>>(() => new Set());

  // Drop decrypted content as soon as the unlock window closes.
  useEffect(() => {
    if (session.unlocked) return;
    setPlaintext(null);
    setShown(new Set());
  }, [session.unlocked]);

  /** The full body; asks for Touch ID only when the unlock window is closed. */
  const fullBody = async (): Promise<string> => {
    if (!item.secure && !item.hasSecrets) return item.body ?? "";
    if (plaintext !== null) return plaintext;
    const body = await session.withUnlock(() => api.revealItem(item.id));
    setPlaintext(body);
    return body;
  };

  const toggleSecret = (index: number) =>
    run(async () => {
      if (!shown.has(index)) await fullBody();
      setShown((prev) => {
        const next = new Set(prev);
        if (!next.delete(index)) next.add(index);
        return next;
      });
    });

  const copySecret = (index: number) =>
    run(async () => {
      const body = await fullBody();
      const value = item.secure ? body : secretValues(body)[index];
      if (value !== undefined) await onCopy(value);
    });

  const copyAll = () => run(async () => onCopy(toPlainText(await fullBody())));

  const segments = item.secure
    ? parseCopyable(plaintext ?? "")
    : item.hasSecrets && plaintext !== null
      ? parseBody(plaintext)
      : parseTemplate(item.body ?? "");
  const bodyHidden = item.secure && !shown.has(WHOLE_BODY);

  return (
    <div className={styles.body}>
      <div className={styles.bodyToolbar}>
        {item.secure && (
          <Button
            variant="ghost"
            size="sm"
            icon={bodyHidden ? Eye : EyeOff}
            onClick={() => toggleSecret(WHOLE_BODY)}
          >
            {bodyHidden ? "Unhide" : "Hide"}
          </Button>
        )}
        <Button variant="ghost" size="sm" icon={Copy} onClick={copyAll}>
          Copy all
        </Button>
      </div>
      {bodyHidden ? (
        <div className={styles.hidden}>
          <span className={styles.mask}>
            <LockKeyhole size={12} strokeWidth={2} />
            hidden
          </span>
        </div>
      ) : (
        <InlineBody
          segments={segments}
          shown={shown}
          onToggleSecret={toggleSecret}
          onCopySecret={copySecret}
          onCopyText={(text) => void run(() => onCopy(text))}
        />
      )}
    </div>
  );
}
