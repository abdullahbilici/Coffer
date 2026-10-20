import { Copy, Eye, EyeOff } from "lucide-react";
import { cx } from "../../lib/cx";
import styles from "./Chips.module.css";

type Props = {
  /** `null` until the secret has been fetched. */
  value: string | null;
  shown: boolean;
  onToggle: () => void;
  onCopy: () => void;
  className?: string;
};

/**
 * A secret (`{{value}}`): eye on the left in both states, "hidden" and a copy button while
 * masked. Once unmasked, clicking the value copies it.
 */
export function SecretChip({ value, shown, onToggle, onCopy, className }: Props) {
  const visible = shown && value !== null;
  const Eyes = visible ? EyeOff : Eye;
  return (
    <span className={cx(styles.secret, className)}>
      <button
        type="button"
        className={styles.action}
        aria-label={visible ? "Hide" : "Unhide"}
        onClick={onToggle}
      >
        {/* Re-keyed on every toggle so the icon crossfades instead of snapping. */}
        <Eyes key={String(visible)} size={13} strokeWidth={2} className={styles.swap} />
      </button>
      {visible ? (
        <button type="button" className={styles.value} onClick={onCopy}>
          {value}
        </button>
      ) : (
        <>
          <span className={styles.masked}>hidden</span>
          <button type="button" className={styles.action} aria-label="Copy" onClick={onCopy}>
            <Copy size={13} strokeWidth={2} />
          </button>
        </>
      )}
    </span>
  );
}
