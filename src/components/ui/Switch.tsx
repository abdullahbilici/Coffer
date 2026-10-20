import styles from "./Switch.module.css";

type Props = {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: string;
};

export function Switch({ checked, onChange, label, description }: Props) {
  return (
    <label className={styles.root}>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        className={styles.track}
        onClick={() => onChange(!checked)}
      >
        <span className={styles.thumb} />
      </button>
      <span className={styles.text}>
        <span className={styles.label}>{label}</span>
        {description && <span className={styles.description}>{description}</span>}
      </span>
    </label>
  );
}
