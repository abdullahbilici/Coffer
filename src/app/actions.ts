import type { ItemSummary } from "../lib/api";

/** Runs an async action and reports failures as a toast. */
export type Run = (action: () => Promise<unknown>) => Promise<void>;

/** Mutations and navigation the sidebar and views can trigger. Implemented in `App`. */
export type VaultActions = {
  selectProject: (id: number) => void;
  selectFolder: (id: number) => void;
  selectTag: (path: string) => void;
  openItem: (item: ItemSummary) => void;
  newItem: (projectId: number, folderId: number | null) => void;

  createProject: (name: string) => void;
  renameProject: (id: number, name: string) => void;
  deleteProject: (id: number) => void;

  /** Opens the inline name field for a new folder in the sidebar. */
  startCreateFolder: (projectId: number, parentId: number | null) => void;
  createFolder: (projectId: number, parentId: number | null, name: string) => void;
  renameFolder: (id: number, name: string) => void;
  deleteFolder: (id: number) => void;

  duplicateItem: (id: number) => void;
  deleteItem: (id: number) => void;
};
