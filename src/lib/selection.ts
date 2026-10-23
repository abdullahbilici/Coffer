/** What the main pane is showing. */
export type Selection =
  | { kind: "project"; id: number }
  | { kind: "folder"; id: number }
  | { kind: "item"; id: number }
  | { kind: "newItem"; projectId: number; folderId: number | null }
  | { kind: "tag"; path: string }
  | { kind: "todos" }
  | { kind: "payments" }
  | { kind: "assistant" }
  /** `id` null creates; `projectId` presets the project of a new one. */
  | { kind: "todoEditor"; id: number | null; projectId: number | null }
  | { kind: "paymentEditor"; id: number | null; projectId: number | null }
  | null;

/** Editors aren't places to go back to. */
export const isEditor = (selection: Selection): boolean =>
  selection?.kind === "newItem" ||
  selection?.kind === "todoEditor" ||
  selection?.kind === "paymentEditor";

/** Keys for expandable sidebar nodes. Everything starts collapsed. */
export const nodeKey = {
  project: (id: number) => `project:${id}`,
  folder: (id: number) => `folder:${id}`,
  item: (id: number) => `item:${id}`,
  tag: (path: string) => `tag:${path}`,
};

/** Keys for agenda rows animating out (see `deleting` in `useVaultActions`). */
export const agendaKey = {
  todo: (id: number) => `todo:${id}`,
  payment: (id: number) => `payment:${id}`,
};

export type TreeSection = "projects" | "tags";

export function inSection(key: string, section: TreeSection): boolean {
  return section === "tags" ? key.startsWith("tag:") : !key.startsWith("tag:");
}
