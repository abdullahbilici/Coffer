import type { ReactNode } from "react";
import styles from "./Settings.module.css";

/** A label on the left and its control on the right. */
export function SettingRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className={styles.row}>
      <span className={styles.label}>{label}</span>
      <div className={styles.control}>{children}</div>
    </div>
  );
}
