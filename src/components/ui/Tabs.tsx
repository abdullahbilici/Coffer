import styles from "./Tabs.module.css";

type Option<T extends string> = { value: T; label: string; count?: number };

type Props<T extends string> = {
  value: T;
  options: Option<T>[];
  onChange: (value: T) => void;
  label: string;
};

/** Text segmented control with optional counts, e.g. a list filter. */
export function Tabs<T extends string>({ value, options, onChange, label }: Props<T>) {
  return (
    <div className={styles.root} role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          className={styles.option}
          onClick={() => onChange(option.value)}
        >
          {option.label}
          {option.count !== undefined && <span className={styles.count}>{option.count}</span>}
        </button>
      ))}
    </div>
  );
}
