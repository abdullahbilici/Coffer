/** What the main pane is showing. */
export type Selection =
  | { kind: "project"; id: number }
  | { kind: "folder"; id: number }
  | { kind: "item"; id: number }
  | { kind: "newItem"; projectId: number; folderId: number | null }
  | { kind: "tag"; path: string }
  | null;

/** Keys for expandable sidebar nodes. Everything starts collapsed. */
export const nodeKey = {
  project: (id: number) => `project:${id}`,
  folder: (id: number) => `folder:${id}`,
  tag: (path: string) => `tag:${path}`,
};

export type TreeSection = "projects" | "tags";

export function inSection(key: string, section: TreeSection): boolean {
  return section === "tags" ? key.startsWith("tag:") : !key.startsWith("tag:");
}
