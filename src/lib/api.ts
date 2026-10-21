import { invoke } from "@tauri-apps/api/core";

/** Mirrors `AppError` in `src-tauri/src/error.rs`. */
export type ErrorKind =
  "locked" | "notFound" | "invalid" | "auth" | "crypto" | "keychain" | "database";

export type AppError = { kind: ErrorKind; message: string };

export type SessionInfo = {
  entered: boolean;
  /** Unix millis when the unlock window ends, if one is open. */
  expiresAt: number | null;
};

export type ProjectSummary = { id: number; name: string };

export type Project = ProjectSummary & {
  /** Markdown; with inline secrets, a `` `!0!` ``-marker template. */
  readme: string;
  hasSecrets: boolean;
  updatedAt: number;
};

export type Folder = {
  id: number;
  projectId: number;
  parentId: number | null;
  name: string;
  createdAt: number;
};

export type PinKind = "project" | "folder" | "item";

export type PinRef = { kind: PinKind; id: number };

export type ItemSummary = {
  id: number;
  projectId: number;
  folderId: number | null;
  title: string;
  /** The whole body is encrypted. */
  secure: boolean;
  /** A plain body with `` `!...!` `` spans encrypted separately. */
  hasSecrets: boolean;
  tags: string[];
  updatedAt: number;
  createdAt: number;
};

/** `body` is `null` for secure items and a `` `!0!` ``-marker template for items with inline secrets. */
export type Item = ItemSummary & { body: string | null };

export type Tree = {
  projects: ProjectSummary[];
  folders: Folder[];
  items: ItemSummary[];
  /** Pinned projects, folders and items in the user's order. */
  pins: PinRef[];
};

export type SaveItemInput = {
  id: number | null;
  projectId: number;
  folderId: number | null;
  title: string;
  body: string;
  secure: boolean;
  tags: string[];
};

export function isAppError(error: unknown): error is AppError {
  return typeof error === "object" && error !== null && "kind" in error && "message" in error;
}

export function isLocked(error: unknown): boolean {
  return isAppError(error) && error.kind === "locked";
}

export function errorMessage(error: unknown): string {
  if (isAppError(error) || error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Something went wrong";
}

export const api = {
  sessionInfo: () => invoke<SessionInfo>("session_info"),
  unlock: () => invoke<SessionInfo>("unlock"),
  lock: () => invoke<SessionInfo>("lock"),

  getTree: () => invoke<Tree>("get_tree"),

  getProject: (id: number) => invoke<Project>("get_project", { id }),
  createProject: (name: string) => invoke<Project>("create_project", { name }),
  renameProject: (id: number, name: string) => invoke<void>("rename_project", { id, name }),
  updateProjectReadme: (id: number, readme: string) =>
    invoke<void>("update_project_readme", { id, readme }),
  revealProjectReadme: (id: number) => invoke<string>("reveal_project_readme", { id }),
  deleteProject: (id: number) => invoke<void>("delete_project", { id }),

  createFolder: (projectId: number, parentId: number | null, name: string) =>
    invoke<number>("create_folder", { projectId, parentId, name }),
  renameFolder: (id: number, name: string) => invoke<void>("rename_folder", { id, name }),
  moveFolder: (id: number, projectId: number, parentId: number | null) =>
    invoke<void>("move_folder", { id, projectId, parentId }),
  deleteFolder: (id: number) => invoke<void>("delete_folder", { id }),

  getItem: (id: number) => invoke<Item>("get_item", { id }),
  revealItem: (id: number) => invoke<string>("reveal_item", { id }),
  saveItem: (input: SaveItemInput) => invoke<ItemSummary>("save_item", { input }),
  duplicateItem: (id: number) => invoke<ItemSummary>("duplicate_item", { id }),
  moveItem: (id: number, projectId: number, folderId: number | null) =>
    invoke<void>("move_item", { id, projectId, folderId }),
  deleteItem: (id: number) => invoke<void>("delete_item", { id }),

  setPinned: (pin: PinRef, pinned: boolean) => invoke<void>("set_pinned", { pin, pinned }),
  reorderPins: (order: PinRef[]) => invoke<void>("reorder_pins", { order }),
};
