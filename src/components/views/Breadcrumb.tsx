import { Fragment } from "react";
import { ChevronRight } from "lucide-react";
import type { VaultActions } from "../../app/actions";
import type { Tree } from "../../lib/api";
import { cx } from "../../lib/cx";
import { nodeKey } from "../../lib/selection";
import { folderPath } from "../../lib/tree";
import styles from "./Breadcrumb.module.css";

export type Crumb = {
  key: string;
  label: string;
  /** Omit for the current place: it renders emphasised and isn't clickable. */
  onClick?: () => void;
};

/**
 * Crumbs from a project root down to `folderId`, each navigating there. With `current`, the
 * last crumb is the page you're on and isn't a link.
 */
export function pathCrumbs(
  tree: Tree,
  projectId: number,
  folderId: number | null,
  actions: VaultActions,
  { current = false } = {},
): Crumb[] {
  const project = tree.projects.find((p) => p.id === projectId);
  const crumbs: Crumb[] = [
    {
      key: nodeKey.project(projectId),
      label: project?.name ?? "",
      onClick: () => actions.selectProject(projectId),
    },
    ...folderPath(tree.folders, folderId).map((f) => ({
      key: nodeKey.folder(f.id),
      label: f.name,
      onClick: () => actions.selectFolder(f.id),
    })),
  ];
  if (current) crumbs[crumbs.length - 1] = { ...crumbs[crumbs.length - 1], onClick: undefined };
  return crumbs;
}

type Props = {
  crumbs: Crumb[];
  /** `trail` sits above a title; `path` is the page header itself; `inline` sits in a row. */
  variant?: "trail" | "path" | "inline";
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
              strokeWidth={1.75}
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
