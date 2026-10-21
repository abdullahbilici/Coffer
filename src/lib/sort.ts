import type { Folder, ItemSummary } from "./api";

export type SortOrder = "name" | "updated" | "created";

const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base" });

/** Pinned entries first, then the chosen order. Notes sort by any order; folders have no
 * edit time, so "updated" keeps them by name. */
export function sortItems(items: ItemSummary[], order: SortOrder, pinned: (id: number) => boolean) {
  const compare = (a: ItemSummary, b: ItemSummary) =>
    order === "updated"
      ? b.updatedAt - a.updatedAt
      : order === "created"
        ? b.createdAt - a.createdAt
        : byName(a.title, b.title);
  return [...items].sort((a, b) => Number(pinned(b.id)) - Number(pinned(a.id)) || compare(a, b));
}

export function sortFolders(folders: Folder[], order: SortOrder, pinned: (id: number) => boolean) {
  const compare = (a: Folder, b: Folder) =>
    order === "created" ? b.createdAt - a.createdAt : byName(a.name, b.name);
  return [...folders].sort((a, b) => Number(pinned(b.id)) - Number(pinned(a.id)) || compare(a, b));
}
