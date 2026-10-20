import type { LucideIcon } from "lucide-react";
import { TagList } from "./TagList";
import styles from "./EntryList.module.css";

export type Entry = {
  key: string;
  icon: LucideIcon;
  title: string;
  subtitle?: string;
  tags?: string[];
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
      {entries.map(({ key, icon: Icon, title, subtitle, tags, onOpen }) => (
        <li key={key} className={styles.row} onClick={onOpen}>
          <Icon size={16} strokeWidth={1.75} className={styles.icon} />
          <div className={styles.text}>
            <span className={styles.title}>{title}</span>
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
