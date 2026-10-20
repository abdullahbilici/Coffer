import { Tag } from "lucide-react";
import styles from "./TagList.module.css";

export function TagList({ tags, onSelect }: { tags: string[]; onSelect: (path: string) => void }) {
  return (
    <ul className={styles.list}>
      {tags.map((tag) => (
        <li key={tag}>
          <button
            type="button"
            className={styles.tag}
            onClick={(event) => {
              event.stopPropagation();
              onSelect(tag);
            }}
          >
            <Tag size={12} strokeWidth={1.75} />
            {tag}
          </button>
        </li>
      ))}
    </ul>
  );
}
