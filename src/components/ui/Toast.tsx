import { Check, CircleAlert, X } from "lucide-react";
import type { ToastState } from "../../hooks/useToast";
import { cx } from "../../lib/cx";
import styles from "./Toast.module.css";

type Props = {
  toast: ToastState | null;
  onDismiss: () => void;
};

/** Bottom-centre of the window, regardless of the sidebar. */
export function Toast({ toast, onDismiss }: Props) {
  if (!toast) return null;
  const Icon = toast.tone === "success" ? Check : CircleAlert;
  return (
    <div
      // A new id restarts the enter animation when a toast replaces another.
      key={toast.id}
      role={toast.tone === "error" ? "alert" : "status"}
      className={cx(styles.toast, toast.leaving && styles.leaving)}
    >
      <Icon size={16} strokeWidth={2} className={styles[toast.tone]} />
      <span className={styles.message}>{toast.message}</span>
      {toast.tone === "error" && (
        <button type="button" className={styles.close} aria-label="Dismiss" onClick={onDismiss}>
          <X size={14} strokeWidth={1.75} />
        </button>
      )}
    </div>
  );
}
