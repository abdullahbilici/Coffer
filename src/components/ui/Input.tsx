import type { ComponentProps } from "react";
import { cx } from "../../lib/cx";
import styles from "./Input.module.css";

export function Input({ className, ...rest }: ComponentProps<"input">) {
  return <input className={cx(styles.field, className)} spellCheck={false} {...rest} />;
}

export function TextArea({ className, ...rest }: ComponentProps<"textarea">) {
  return (
    <textarea className={cx(styles.field, styles.area, className)} spellCheck={false} {...rest} />
  );
}
