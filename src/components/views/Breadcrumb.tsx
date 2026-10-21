import { Fragment } from "react";
import { ChevronRight } from "lucide-react";
import type { VaultActions } from "../../app/actions";
import type { Folder, ProjectSummary } from "../../lib/api";
import styles from "./Breadcrumb.module.css";

type Props = {
  project: ProjectSummary;
  /** Folders from the project root down, each clickable. */
  folders: Folder[];
  actions: VaultActions;
};

/** Clickable "Project > Folder > Sub" trail above a page title: navigates up the hierarchy. */
export function Breadcrumb({ project, folders, actions }: Props) {
  return (
    <nav className={styles.breadcrumb} aria-label="Location">
      <button
        type="button"
        className={styles.crumb}
        onClick={() => actions.selectProject(project.id)}
      >
        {project.name}
      </button>
      {folders.map((folder) => (
        <Fragment key={folder.id}>
          <ChevronRight size={12} strokeWidth={2} className={styles.separator} />
          <button
            type="button"
            className={styles.crumb}
            onClick={() => actions.selectFolder(folder.id)}
          >
            {folder.name}
          </button>
        </Fragment>
      ))}
    </nav>
  );
}
