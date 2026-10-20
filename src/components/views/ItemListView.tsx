import type { LucideIcon } from "lucide-react";
import type { ItemSummary, Tree } from "../../lib/api";
import { locationLabel } from "../../lib/tree";
import { itemIcon } from "../itemIcon";
import { EntryList } from "./EntryList";
import viewStyles from "./View.module.css";

type Props = {
  icon: LucideIcon;
  title: string;
  items: ItemSummary[];
  tree: Tree;
  onOpen: (item: ItemSummary) => void;
  onSelectTag: (path: string) => void;
};

/** Flat list of items, used for tag filters and search results. */
export function ItemListView({ icon: Icon, title, items, tree, onOpen, onSelectTag }: Props) {
  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <header className={viewStyles.header}>
          <div>
            <div className={viewStyles.eyebrow}>
              <Icon size={14} strokeWidth={1.75} />
              {items.length} {items.length === 1 ? "item" : "items"}
            </div>
            <h1 className={viewStyles.title}>{title}</h1>
          </div>
        </header>

        <EntryList
          emptyText="No items"
          onSelectTag={onSelectTag}
          entries={items.map((item) => ({
            key: `item:${item.id}`,
            icon: itemIcon(item),
            title: item.title,
            subtitle: locationLabel(tree, item),
            tags: item.tags,
            onOpen: () => onOpen(item),
          }))}
        />
      </div>
    </div>
  );
}
