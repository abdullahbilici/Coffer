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
 * A secret (`` `!value!` ``): eye on the left in both states, "hidden" and a copy button while
 * masked. Eye and "hidden" form one unhide button; only the copy button copies. Once unmasked,
 * clicking the value copies it.
 */
export function SecretChip({ value, shown, onToggle, onCopy, className }: Props) {
  const visible = shown && value !== null;
  const Eyes = visible ? EyeOff : Eye;
  return (
    <span className={cx(styles.secret, className)}>
      {/* One button for eye + "hidden", so the eye never moves between states. */}
      <button
        type="button"
        className={styles.toggle}
        aria-label={visible ? "Hide" : "Unhide"}
        onClick={onToggle}
      >
        {/* Re-keyed on every toggle so the icon crossfades instead of snapping. */}
        <Eyes key={String(visible)} size={14} strokeWidth={1.75} className={styles.swap} />
        {!visible && <span className={styles.masked}>hidden</span>}
      </button>
      {visible ? (
        <button type="button" className={styles.value} onClick={onCopy}>
          {value}
        </button>
      ) : (
        <button type="button" className={styles.copy} aria-label="Copy" onClick={onCopy}>
          <Copy size={14} strokeWidth={1.75} />
        </button>
      )}
    </span>
  );
}
