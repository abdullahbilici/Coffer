import { Folder, Pin } from "lucide-react";
import type { Folder as FolderModel } from "../../lib/api";
import styles from "./FolderGrid.module.css";

type Props = {
  folders: FolderModel[];
  /** Short summary under each name, e.g. "3 items, 1 folder". */
  describe: (folder: FolderModel) => string;
  isPinned: (folder: FolderModel) => boolean;
  onOpen: (folder: FolderModel) => void;
};

/** Subfolders as tiles above the note list, so the two never read as the same thing. */
export function FolderGrid({ folders, describe, isPinned, onOpen }: Props) {
  return (
    <ul className={styles.grid}>
      {folders.map((folder) => (
        <li key={folder.id}>
          <button type="button" className={styles.card} onClick={() => onOpen(folder)}>
            <span className={styles.iconWrap}>
              <Folder size={18} strokeWidth={1.75} />
            </span>
            <span className={styles.text}>
              <span className={styles.name}>
                {folder.name}
                {isPinned(folder) && (
                  <Pin size={12} strokeWidth={2} className={styles.pin} aria-label="Pinned" />
                )}
              </span>
              <span className={styles.meta}>{describe(folder)}</span>
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}
