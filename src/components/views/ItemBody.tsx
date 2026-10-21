import { useEffect, useState, type ReactNode } from "react";
import { Copy, Eye, EyeOff, LockKeyhole } from "lucide-react";
import type { Run } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { api, type Item } from "../../lib/api";
import { secretValues, toPlainText } from "../../lib/markup";
import { Button } from "../ui/Button";
import { Markdown, type MarkdownChips } from "./Markdown";
import styles from "./ItemBody.module.css";

type Props = {
  /** Re-mount (key on `updatedAt`) to reset revealed state after the item changes. */
  item: Item;
  session: Session;
  run: Run;
  /** Writes to the clipboard and confirms with a toast. */
  onCopy: (text: string) => Promise<void>;
  /** Shown at the left of the toolbar on the note page; previews omit it. */
  title?: ReactNode;
};

/** Secret index used for the whole body of a secure item. */
const WHOLE_BODY = 0;

/**
 * An item's body with Copy all, chips and secrets. Used by the item page and search previews.
 * Secrets open masked; decrypted text lives only as long as this component.
 */
export function ItemBody({ item, session, run, onCopy, title }: Props) {
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

  // Notes render as markdown. Inline secrets always come from the stored template's markers,
  // filled from the decrypted body once fetched; a secure body is rendered as-is once shown.
  const source = item.secure ? (plaintext ?? "") : (item.body ?? "");
  const chips: MarkdownChips = {
    secrets: item.hasSecrets && plaintext !== null ? secretValues(plaintext) : null,
    shown,
    onToggleSecret: toggleSecret,
    onCopySecret: copySecret,
    onCopyText: (text) => void run(() => onCopy(text)),
  };
  const bodyHidden = item.secure && !shown.has(WHOLE_BODY);

  return (
    <div className={styles.body}>
      <div className={styles.bodyToolbar}>
        {title && <h1 className={styles.title}>{title}</h1>}
        <span className={styles.spacer} />
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
          <button type="button" className={styles.mask} onClick={() => toggleSecret(WHOLE_BODY)}>
            <LockKeyhole size={12} strokeWidth={2} />
            hidden
          </button>
        </div>
      ) : (
        <div className={styles.content}>
          <Markdown source={source} chips={chips} size="md" />
        </div>
      )}
    </div>
  );
}
