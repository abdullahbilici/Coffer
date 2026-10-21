import type { LucideIcon } from "lucide-react";
import { cx } from "../../lib/cx";
import { TagList } from "./TagList";
import { PinBadge } from "../ui/PinBadge";
import styles from "./EntryList.module.css";

export type Entry = {
  key: string;
  icon: LucideIcon;
  title: string;
  subtitle?: string;
  tags?: string[];
  /** Draws the icon on a tinted tile; used for folders so they stand apart from notes. */
  boxed?: boolean;
  pinned?: boolean;
  onOpen: () => void;
};

type Props = {
  entries: Entry[];
  emptyText: string;
  onSelectTag: (path: string) => void;
};

/** Bordered list of clickable rows, shared by folder, tag and search views. */
export function EntryList({ entries, emptyText, onSelectTag }: Props) {
  if (entries.length === 0) return <p className={styles.empty}>{emptyText}</p>;
  return (
    <ul className={styles.list}>
      {entries.map(({ key, icon: Icon, title, subtitle, tags, boxed, pinned, onOpen }) => (
        <li key={key} className={styles.row} onClick={onOpen}>
          {/* Every icon gets the same slot, so boxed and plain rows keep their titles aligned. */}
          <span className={cx(styles.iconSlot, boxed && styles.boxed)}>
            <Icon size={16} strokeWidth={1.75} />
          </span>
          <div className={styles.text}>
            <span className={styles.title}>
              {title}
              {pinned && <PinBadge className={styles.pin} />}
            </span>
            {subtitle && <span className={styles.location}>{subtitle}</span>}
          </div>
          {tags && tags.length > 0 && (
            <div className={styles.tags}>
              <TagList tags={tags} onSelect={onSelectTag} />
            </div>
          )}
        </li>
      ))}
    </ul>
  );
}
