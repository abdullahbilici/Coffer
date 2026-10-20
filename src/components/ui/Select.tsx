import { ChevronDown } from "lucide-react";
import styles from "./Select.module.css";

type Option<T> = { value: T; label: string };

type Props<T> = {
  value: T;
  options: Option<T>[];
  onChange: (value: T) => void;
};

/** Native select so the macOS popup is used; options are matched by index. */
export function Select<T>({ value, options, onChange }: Props<T>) {
  const index = options.findIndex((o) => o.value === value);
  return (
    <div className={styles.root}>
      <select
        className={styles.select}
        value={index}
        onChange={(e) => onChange(options[Number(e.target.value)].value)}
      >
        {options.map((option, i) => (
          <option key={i} value={i}>
            {option.label}
          </option>
        ))}
      </select>
      <ChevronDown size={14} strokeWidth={1.75} className={styles.chevron} />
    </div>
  );
}
