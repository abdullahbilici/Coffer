import type { ButtonHTMLAttributes } from "react";
import type { LucideIcon } from "lucide-react";
import { cx } from "../../lib/cx";
import styles from "./IconButton.module.css";

type Props = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> & {
  icon: LucideIcon;
  /** Used for the tooltip and screen readers. */
  label: string;
  size?: "sm" | "md";
};

export function IconButton({ icon: Icon, label, size = "md", className, ...rest }: Props) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={cx(styles.button, styles[size], className)}
      {...rest}
    >
      <Icon size={size === "sm" ? 14 : 16} strokeWidth={1.75} />
    </button>
  );
}
