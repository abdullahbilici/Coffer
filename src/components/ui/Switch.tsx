import styles from "./Switch.module.css";

type Props = {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: string;
  /** Only screen readers get the label, e.g. when a settings row already shows it. */
  labelHidden?: boolean;
};

export function Switch({ checked, onChange, label, description, labelHidden }: Props) {
  return (
    <label className={styles.root}>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={labelHidden ? label : undefined}
        className={styles.track}
        onClick={() => onChange(!checked)}
      >
        <span className={styles.thumb} />
      </button>
      {!labelHidden && (
        <span className={styles.text}>
          <span className={styles.label}>{label}</span>
          {description && <span className={styles.description}>{description}</span>}
        </span>
      )}
    </label>
  );
}
