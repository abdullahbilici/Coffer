import { invoke } from "@tauri-apps/api/core";

/** Mirrors `AppError` in `src-tauri/src/error.rs`. */
export type ErrorKind =
  | "locked"
  | "notFound"
  | "invalid"
  | "auth"
  | "crypto"
  | "keychain"
  | "file"
  | "network"
  | "database";

export type AppError = { kind: ErrorKind; message: string };

export type SessionInfo = {
  entered: boolean;
  unlocked: boolean;
  /** Unix millis when the unlock window ends; `null` while locked or with no timer. */
  expiresAt: number | null;
};

export type BackupSettings = {
  /** A backup passphrase has been set. */
  configured: boolean;
  auto: boolean;
  dir: string | null;
  lastAt: number | null;
  lastError: string | null;
};

export type AssistantSettings = {
  /** Base URL of an Ollama server. */
  url: string;
  model: string;
  /** What the assistant should know; it reads this but never edits it. */
  context: string;
  /** An API key is stored in the Keychain. */
  hasApiKey: boolean;
};

/** Mirrors `db::settings::Settings`. */
export type Settings = {
  /** `null`: until the screen locks. */
  unlockWindowSecs: number | null;
  /** `null`: never. */
  clipboardClearSecs: number | null;
  backup: BackupSettings;
  assistant: AssistantSettings;
  /** `coffer --mcp` may answer an MCP client. */
  mcpAccess: boolean;
  /** Opens quick capture from any app, e.g. `Alt+Shift+Space` (see `lib/shortcut.ts`). */
  captureShortcut: string | null;
};

export type Conversation = {
  id: number;
  title: string;
  createdAt: number;
  updatedAt: number;
  /** A reply is being written right now. */
  replying: boolean;
};

export type ToolCall = { id: string; name: string; arguments: Record<string, unknown> };

/** A value in a draft: the user's own (masked when secret), or a stored secret's handle. */
export type DraftValue = { text: string; secret: boolean } | { handle: string };

/** Mirrors `assistant::draft::Line`. */
export type DraftLine = {
  sign: "add" | "change" | "remove";
  label: string;
  value: DraftValue | null;
};

/** Mirrors `assistant::draft::Entry`. */
export type DraftEntry = {
  kind: "project" | "folder" | "note" | "todo" | "payment";
  action: "add" | "edit" | "rename" | "move" | "delete" | "done" | "cancel";
  title: string;
  detail: string | null;
  lines: DraftLine[];
};

/** Mirrors `assistant::draft::Group`: the changes to one project. */
export type DraftGroup = { project: string; new: boolean; entries: DraftEntry[] };

export type DraftStatus = "pending" | "applied" | "discarded";

/** Mirrors `assistant::Turn`. Assistant text may hold secret handles like `[n12.0]`. */
export type Turn =
  | { role: "user"; text: string; paste?: string }
  | { role: "assistant"; text: string; toolCalls: ToolCall[]; stopped: boolean }
  | { role: "tool"; callId: string; name: string; result: string }
  | { role: "draft"; groups: DraftGroup[]; status: DraftStatus };

export type Message = Turn & { id: number; createdAt: number };

/** Mirrors `commands::assistant::AssistantEvent`. */
export type AssistantEvent =
  /** The reply's text so far. */
  | { type: "text"; conversationId: number; text: string }
  | { type: "message"; conversationId: number; message: Message }
  | { type: "done"; conversationId: number }
  | { type: "failed"; conversationId: number; message: string };

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

/** A local calendar day, `YYYY-MM-DD`. */
export type DateString = string;

export type Todo = {
  id: number;
  /** `null` for a general todo. */
  projectId: number | null;
  /** An optional linked note. */
  itemId: number | null;
  title: string;
  /** Markdown; with inline secrets, a `` `!0!` ``-marker template. */
  description: string;
  hasSecrets: boolean;
  dueDate: DateString | null;
  done: boolean;
  createdAt: number;
  updatedAt: number;
};

export type Cycle = "monthly" | "yearly" | "once";

/** An amount in integer hundredths, due from `from` on. */
export type Price = { from: DateString; amount: number };

export type Payment = {
  id: number;
  /** `null` for a personal payment. */
  projectId: number | null;
  name: string;
  /** ISO 4217 code, e.g. `USD`. */
  currency: string;
  cycle: Cycle;
  /** The first due date. */
  startDate: DateString;
  /** Nothing is due on or after this day. */
  cancelDate: DateString | null;
  /** Oldest first; never empty. */
  prices: Price[];
  createdAt: number;
  updatedAt: number;
};

export type Tree = {
  projects: ProjectSummary[];
  folders: Folder[];
  items: ItemSummary[];
  /** Pinned projects, folders and items in the user's order. */
  pins: PinRef[];
  todos: Todo[];
  payments: Payment[];
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

export type SaveTodoInput = {
  id: number | null;
  projectId: number | null;
  itemId: number | null;
  title: string;
  /** Full text with `` `!x!` `` secrets; the backend seals them. */
  description: string;
  dueDate: DateString | null;
};

export type SavePaymentInput = {
  id: number | null;
  projectId: number | null;
  name: string;
  currency: string;
  cycle: Cycle;
  startDate: DateString;
  cancelDate: DateString | null;
  /** Required when creating (`from` is then ignored); when editing, records a price change. */
  price: Price | null;
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

  getSettings: () => invoke<Settings>("get_settings"),
  setUnlockWindow: (secs: number | null) => invoke<SessionInfo>("set_unlock_window", { secs }),
  setClipboardClear: (secs: number | null) => invoke<void>("set_clipboard_clear", { secs }),
  clearClipboardLater: () => invoke<void>("clear_clipboard_later"),
  setMcpAccess: (on: boolean) => invoke<void>("set_mcp_access", { on }),
  /** The `claude mcp add` command for the running app. */
  mcpCommand: () => invoke<string>("mcp_command"),
  /** `null` switches quick capture's shortcut off. */
  setCaptureShortcut: (shortcut: string | null) =>
    invoke<void>("set_capture_shortcut", { shortcut }),

  /** Quick capture's own window: hide it, take the keyboard back, fit it to its content. */
  hideCapture: () => invoke<void>("hide_capture"),
  focusCapture: () => invoke<void>("focus_capture"),
  fitCapture: (height: number) => invoke<void>("fit_capture", { height }),
  /** Brings the main window forward on a conversation. */
  openInCoffer: (conversationId: number) => invoke<void>("open_in_coffer", { conversationId }),

  /** `apiKey`: `null` keeps the stored key, `""` removes it. */
  saveAssistantSettings: (url: string, model: string, context: string, apiKey: string | null) =>
    invoke<Settings>("save_assistant_settings", { url, model, context, apiKey }),
  /** Uses the stored API key only when `url` is the saved one and no key is typed. */
  assistantModels: (url: string, apiKey: string | null) =>
    invoke<string[]>("assistant_models", { url, apiKey }),
  listConversations: () => invoke<Conversation[]>("list_conversations"),
  getConversation: (id: number) => invoke<Message[]>("get_conversation", { id }),
  renameConversation: (id: number, title: string) =>
    invoke<void>("rename_conversation", { id, title }),
  deleteConversation: (id: number) => invoke<void>("delete_conversation", { id }),
  /** Starts a reply in the background; it arrives as `assistant-event`s. */
  /** `paste`: text to import with the message. */
  sendMessage: (conversationId: number | null, text: string, paste: string | null = null) =>
    invoke<{ conversation: Conversation; message: Message }>("send_message", {
      conversationId,
      text,
      paste,
    }),
  /** Applies a drafted change set in one transaction; returns the draft marked applied. */
  applyDraft: (conversationId: number, messageId: number) =>
    invoke<Message>("apply_draft", { conversationId, messageId }),
  discardDraft: (conversationId: number, messageId: number) =>
    invoke<Message>("discard_draft", { conversationId, messageId }),
  cancelReply: (conversationId: number) => invoke<void>("cancel_reply", { conversationId }),
  /** The value behind a handle the assistant wrote, e.g. `n12.0`. */
  revealHandle: (handle: string) => invoke<string>("reveal_handle", { handle }),

  vaultIsEmpty: () => invoke<boolean>("vault_is_empty"),
  defaultBackupDir: () => invoke<string>("default_backup_dir"),
  suggestPassphrase: () => invoke<string>("suggest_passphrase"),
  setBackupPassphrase: (passphrase: string, dir: string) =>
    invoke<Settings>("set_backup_passphrase", { passphrase, dir }),
  backUpNow: () => invoke<Settings>("back_up_now"),
  setBackupDir: (dir: string) => invoke<Settings>("set_backup_dir", { dir }),
  setBackupAuto: (on: boolean) => invoke<Settings>("set_backup_auto", { on }),
  restoreBackup: (path: string, passphrase: string) =>
    invoke<void>("restore_backup", { path, passphrase }),

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
  /** Ids of notes whose readable body contains `query`; never looks inside secrets. */
  searchItemBodies: (query: string) => invoke<number[]>("search_item_bodies", { query }),

  saveTodo: (input: SaveTodoInput) => invoke<Todo>("save_todo", { input }),
  revealTodo: (id: number) => invoke<string>("reveal_todo", { id }),
  setTodoDone: (id: number, done: boolean) => invoke<void>("set_todo_done", { id, done }),
  deleteTodo: (id: number) => invoke<void>("delete_todo", { id }),

  savePayment: (input: SavePaymentInput) => invoke<Payment>("save_payment", { input }),
  deletePayment: (id: number) => invoke<void>("delete_payment", { id }),

  setPinned: (pin: PinRef, pinned: boolean) => invoke<void>("set_pinned", { pin, pinned }),
  reorderPins: (order: PinRef[]) => invoke<void>("reorder_pins", { order }),
};
