import type { ReactNode } from "react";
import { Copy, Eye, EyeOff, LockKeyhole } from "lucide-react";
import type { Run } from "../../app/actions";
import { useSecrets } from "../../hooks/useSecrets";
import type { Session } from "../../hooks/useSession";
import { api, type Item } from "../../lib/api";
import { Button } from "../ui/Button";
import { Markdown } from "./Markdown";
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
  const { plaintext, shown, toggle, copyAll, chips } = useSecrets({
    readable: item.body ?? "",
    protected: item.secure || item.hasSecrets,
    wholeBody: item.secure,
    reveal: () => api.revealItem(item.id),
    session,
    run,
    onCopy,
  });

  // Notes render as markdown. Inline secrets come from the stored template's markers, filled
  // from the decrypted body once fetched; a secure body is rendered as-is once shown.
  const source = item.secure ? (plaintext ?? "") : (item.body ?? "");
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
            onClick={() => toggle(WHOLE_BODY)}
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
          <button type="button" className={styles.mask} onClick={() => toggle(WHOLE_BODY)}>
            <LockKeyhole size={12} strokeWidth={1.75} />
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
