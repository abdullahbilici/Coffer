import type { ItemSummary, PinRef } from "../lib/api";

/** Runs an async action and reports failures as a toast. */
export type Run = (action: () => Promise<unknown>) => Promise<void>;

/** Mutations and navigation the sidebar and views can trigger. Implemented in `App`. */
export type VaultActions = {
  selectProject: (id: number) => void;
  selectFolder: (id: number) => void;
  selectTag: (path: string) => void;
  openItem: (item: ItemSummary) => void;
  /** Runs `then` once the unlock window is open, asking for Touch ID if needed. */
  guard: (then: () => void) => void;
  newItem: (projectId: number, folderId: number | null) => void;

  createProject: (name: string) => void;
  renameProject: (id: number, name: string) => void;
  deleteProject: (id: number) => void;

  /** Opens the inline name field for a new folder in the sidebar. */
  startCreateFolder: (projectId: number, parentId: number | null) => void;
  createFolder: (projectId: number, parentId: number | null, name: string) => void;
  renameFolder: (id: number, name: string) => void;
  deleteFolder: (id: number) => void;
  moveFolder: (id: number, projectId: number, parentId: number | null) => void;

  duplicateItem: (id: number) => void;
  setPinned: (pin: PinRef, pinned: boolean) => void;
  /** Resolves once the tree has reloaded with the new order. */
  reorderPins: (order: PinRef[]) => Promise<void>;
  deleteItem: (id: number) => void;
  moveItem: (id: number, projectId: number, folderId: number | null) => void;
};
