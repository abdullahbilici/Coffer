import type { ItemSummary } from "./api";

export type TagNode = {
  /** Last path segment, e.g. "ssh" for "infra/ssh". */
  name: string;
  path: string;
  /** Items tagged with this path or any path below it. */
  count: number;
  children: TagNode[];
};

const SEPARATOR = "/";

/** True when the item has `path` itself or a tag nested below it. */
export function hasTag(item: ItemSummary, path: string): boolean {
  return item.tags.some((tag) => tag === path || tag.startsWith(path + SEPARATOR));
}

/** Groups `a/b`-style tags into a tree, like 1Password's nested tags. */
export function buildTagTree(items: ItemSummary[]): TagNode[] {
  const itemsByPath = new Map<string, Set<number>>();
  for (const item of items) {
    for (const tag of item.tags) {
      const segments = tag.split(SEPARATOR);
      for (let depth = 1; depth <= segments.length; depth++) {
        const path = segments.slice(0, depth).join(SEPARATOR);
        let ids = itemsByPath.get(path);
        if (!ids) itemsByPath.set(path, (ids = new Set()));
        ids.add(item.id);
      }
    }
  }

  const roots: TagNode[] = [];
  const nodes = new Map<string, TagNode>();
  const paths = [...itemsByPath.keys()].sort((a, b) => a.localeCompare(b));
  for (const path of paths) {
    const cut = path.lastIndexOf(SEPARATOR);
    const node: TagNode = {
      name: path.slice(cut + 1),
      path,
      count: itemsByPath.get(path)!.size,
      children: [],
    };
    nodes.set(path, node);
    const parent = cut === -1 ? undefined : nodes.get(path.slice(0, cut));
    (parent?.children ?? roots).push(node);
  }
  return roots;
}
