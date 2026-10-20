import { cx } from "../../lib/cx";
import styles from "./Chips.module.css";

type Props = { text: string; onCopy: (text: string) => void; className?: string };

/** A copyable value (`` `value` ``). Clicking copies it. */
export function CopyChip({ text, onCopy, className }: Props) {
  return (
    <button type="button" className={cx(styles.copyable, className)} onClick={() => onCopy(text)}>
      {text}
    </button>
  );
}
