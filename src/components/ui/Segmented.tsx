import type { LucideIcon } from "lucide-react";
import styles from "./Segmented.module.css";

type Option<T extends string> = { value: T; label: string; icon: LucideIcon };

type Props<T extends string> = {
  value: T;
  options: Option<T>[];
  onChange: (value: T) => void;
};

/** Icon-only segmented control, e.g. grid / list view. */
export function Segmented<T extends string>({ value, options, onChange }: Props<T>) {
  return (
    <div className={styles.root} role="radiogroup">
      {options.map(({ value: v, label, icon: Icon }) => (
        <button
          key={v}
          type="button"
          role="radio"
          aria-checked={v === value}
          aria-label={label}
          title={label}
          className={styles.option}
          onClick={() => onChange(v)}
        >
          <Icon size={14} strokeWidth={1.75} />
        </button>
      ))}
    </div>
  );
}
