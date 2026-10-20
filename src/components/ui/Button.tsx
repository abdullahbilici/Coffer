import type { ButtonHTMLAttributes } from "react";
import type { LucideIcon } from "lucide-react";
import { cx } from "../../lib/cx";
import styles from "./Button.module.css";

type Props = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md";
  icon?: LucideIcon;
};

export function Button({
  variant = "secondary",
  size = "md",
  icon: Icon,
  className,
  children,
  type = "button",
  ...rest
}: Props) {
  return (
    <button
      type={type}
      className={cx(styles.button, styles[variant], styles[size], className)}
      {...rest}
    >
      {Icon && <Icon size={size === "sm" ? 14 : 16} strokeWidth={1.75} />}
      {children}
    </button>
  );
}
