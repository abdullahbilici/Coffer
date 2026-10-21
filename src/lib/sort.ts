import type { Folder, ItemSummary } from "./api";
import { pinnedFirst } from "./pins";

export type SortOrder = "name" | "updated" | "created";

const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base" });

/** Pinned first, then the chosen order. */
export function sortItems(items: ItemSummary[], order: SortOrder, pinned: (id: number) => boolean) {
  const compare = (a: ItemSummary, b: ItemSummary) =>
    order === "updated"
      ? b.updatedAt - a.updatedAt
      : order === "created"
        ? b.createdAt - a.createdAt
        : byName(a.title, b.title);
  return pinnedFirst([...items].sort(compare), (item) => pinned(item.id));
}

/** Folders have no edit time, so "updated" keeps them by name. Pinned first. */
export function sortFolders(folders: Folder[], order: SortOrder, pinned: (id: number) => boolean) {
  const compare = (a: Folder, b: Folder) =>
    order === "created" ? b.createdAt - a.createdAt : byName(a.name, b.name);
  return pinnedFirst([...folders].sort(compare), (folder) => pinned(folder.id));
}
