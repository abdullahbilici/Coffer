import { Fragment } from "react";
import { ChevronRight } from "lucide-react";
import { cx } from "../../lib/cx";
import styles from "./Breadcrumb.module.css";

export type Crumb = {
  key: string;
  label: string;
  /** Omit for the current place: it renders emphasised and isn't clickable. */
  onClick?: () => void;
};

type Props = {
  crumbs: Crumb[];
  /** `trail` sits above a title; `path` is the page header itself. */
  variant?: "trail" | "path";
};

/** "Project > Folder > Sub" trail; each ancestor navigates up the hierarchy. */
export function Breadcrumb({ crumbs, variant = "trail" }: Props) {
  return (
    <nav className={cx(styles.breadcrumb, styles[variant])} aria-label="Location">
      {crumbs.map((crumb, i) => (
        <Fragment key={crumb.key}>
          {i > 0 && (
            <ChevronRight
              size={variant === "path" ? 16 : 12}
              strokeWidth={2}
              className={styles.separator}
            />
          )}
          {crumb.onClick ? (
            <button type="button" className={styles.crumb} onClick={crumb.onClick}>
              {crumb.label}
            </button>
          ) : (
            <span className={cx(styles.crumb, styles.current)} aria-current="page">
              {crumb.label}
            </span>
          )}
        </Fragment>
      ))}
    </nav>
  );
}
