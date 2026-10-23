import type { DraftGroup, DraftStatus, Message, ReplyPhase, ToolCall, Tree } from "./api";

/**
 * A secret handle the assistant writes: `[n12.0]` (note 12, secret 0, or value 0 of a secure
 * note), `[p3.0]` (project overview), `[t4.0]` (todo), `[n12]` (a whole secure note). Mirrors
 * `assistant/handles.rs`. Models sometimes wrap it in backticks; those go too.
 */
const HANDLE = /`?\[([npt]\d+(?:\.\d+)?)\]`?/g;

/**
 * Assistant text as markup: every handle becomes a `` `!k!` `` secret marker, so `Markdown`
 * renders it as a masked chip. `handles[k]` is what to reveal for marker k.
 */
export function withSecretChips(text: string): { source: string; handles: string[] } {
  const handles: string[] = [];
  const source = text.replace(HANDLE, (_, handle: string) => {
    let index = handles.indexOf(handle);
    if (index < 0) index = handles.push(handle) - 1;
    return `\`!${index}!\``;
  });
  return { source, handles };
}

const text = (value: unknown): string | null =>
  typeof value === "string" && value.trim() ? value.trim() : null;

const id = (value: unknown): number | null => {
  const n = typeof value === "number" ? value : Number(text(value)?.replace(/^#/, ""));
  return Number.isInteger(n) ? n : null;
};

/** A calm one-line description of a tool call, e.g. "Searched for aws". */
/** What each tool is, for the status while it runs ("Using search"). */
const TOOL_NAMES: Record<string, string> = {
  list_projects: "projects",
  get_project: "project",
  list_folder: "folder",
  search: "search",
  get_note: "note",
  list_todos: "todos",
  list_payments: "payments",
  project_spend: "spend",
};

/**
 * The quiet status under a reply in progress: the model loading, an import's part, reading
 * the prompt, writing
 * (while nothing visible has streamed yet), or a tool running: a read tool by name, a write
 * tool as drafting. `null` shows nothing.
 */
export function replyStatus(
  messages: Message[],
  phase: ReplyPhase | null,
  streaming: string,
  progress: ImportProgress | null = null,
): string | null {
  if (phase === "loading") return "Loading model";
  if (progress) return `Part ${progress.part} of ${progress.parts}`;
  // The calls of the last model turn, followed by the results that are in so far.
  let at = messages.length - 1;
  while (at >= 0 && messages[at].role === "tool") at--;
  const last = at >= 0 ? messages[at] : undefined;
  if (last?.role === "assistant") {
    const done = new Set(messages.slice(at).flatMap((m) => (m.role === "tool" ? [m.callId] : [])));
    const running = last.toolCalls.find((c) => !done.has(c.id));
    if (running) {
      const name = TOOL_NAMES[running.name];
      return name ? `Using ${name}` : "Drafting";
    }
  }
  if (phase === "reading") return "Reading";
  if (phase === "writing" && !streaming) return "Writing";
  return null;
}

export function toolLabel(call: ToolCall, tree: Tree, result?: string): string {
  const label = describe(call, tree);
  return result?.startsWith("Error:") ? label.replace(/^Drafted /, "Couldn't draft ") : label;
}

function describe(call: ToolCall, tree: Tree): string {
  const args = call.arguments ?? {};
  const withName = (label: string, name: unknown) => {
    const value = text(name);
    return value ? `${label} ${value}` : label;
  };
  const noteTitle = (value: unknown) =>
    tree.items.find((i) => i.id === id(value))?.title ?? "a note";
  const todoTitle = (value: unknown) =>
    tree.todos.find((t) => t.id === id(value))?.title ?? "a todo";
  const paymentName = (value: unknown) =>
    tree.payments.find((p) => p.id === id(value))?.name ?? "a payment";
  const project = text(args.project);
  const inProject = project ? ` for ${project}` : "";
  switch (call.name) {
    case "list_projects":
      return "Listed projects";
    case "get_project":
      return project ? `Opened ${project}` : "Opened a project";
    case "list_folder": {
      const folder = tree.folders.find((f) => f.id === id(args.folder_id));
      return folder ? `Opened ${folder.name}` : "Opened a folder";
    }
    case "search": {
      const query = text(args.query);
      return query ? `Searched for ${query}` : "Searched";
    }
    case "get_note": {
      const item = tree.items.find((i) => i.id === id(args.id));
      return item ? `Read ${item.title}` : "Read a note";
    }
    case "list_todos":
      return `Checked todos${inProject}`;
    case "list_payments":
      return `Checked payments${inProject}`;
    case "project_spend":
      return `Added up spend${inProject}`;
    case "create_project":
      return withName("Drafted project", args.name);
    case "create_folder":
      return withName("Drafted folder", args.name);
    case "create_note":
      return withName("Drafted note", args.title);
    case "add_notes": {
      const count = Array.isArray(args.notes) ? args.notes.length : 0;
      return count === 1 ? "Drafted 1 note" : `Drafted ${count} notes`;
    }
    case "add_field":
    case "update_field":
    case "remove_field":
      return `Drafted ${text(args.label) ?? "a field"} in ${noteTitle(args.note)}`;
    case "rename":
      return withName("Drafted rename to", args.name);
    case "move_note":
      return `Drafted moving ${noteTitle(args.note)}`;
    case "delete_note":
      return `Drafted deleting ${noteTitle(args.note)}`;
    case "create_todo":
      return withName("Drafted todo", args.title);
    case "update_todo":
    case "complete_todo":
      return `Drafted ${todoTitle(args.id)}`;
    case "create_payment":
      return withName("Drafted payment", args.name);
    case "update_payment":
    case "cancel_payment":
      return `Drafted ${paymentName(args.id)}`;
    default:
      return call.name;
  }
}

/** What the thread shows, in order. Tool calls in a row form one group, whichever turns made them. */
export type Block =
  | { kind: "user"; key: string; text: string; paste: string | null }
  | {
      kind: "reply";
      key: string;
      text: string;
      read: Read;
      /** Written next to tool calls, on the way to the answer. */
      aside: boolean;
    }
  | { kind: "tools"; key: string; calls: ToolCall[] }
  | { kind: "stopped"; key: string }
  | {
      kind: "draft";
      key: string;
      id: number;
      groups: DraftGroup[];
      status: DraftStatus;
      /** Still being drafted (an import between parts): not stored, can't be applied yet. */
      building: boolean;
    };

/** Where an import is; `groups` is what the parts so far drafted. */
export type ImportProgress = { part: number; parts: number; groups: DraftGroup[] };

/**
 * The thread as blocks, with an import's draft so far at the end. A draft is keyed by the
 * message it answers, so the stored one takes over the card that was being filled in.
 */
export function threadBlocks(messages: Message[], building: DraftGroup[] = []): Block[] {
  const blocks: Block[] = [];
  let read = emptyRead();
  let asked = 0;
  for (const m of messages) {
    if (m.role === "user") {
      asked = m.id;
      blocks.push({ kind: "user", key: `user:${m.id}`, text: m.text, paste: m.paste ?? null });
      read = emptyRead();
    }
    if (m.role === "tool") {
      for (const match of m.result.matchAll(NOTE_LINE)) read.notes.add(Number(match[1]));
    }
    if (m.role === "draft") {
      const { groups, status } = m;
      blocks.push({
        kind: "draft",
        key: `draft:${asked}`,
        id: m.id,
        groups,
        status,
        building: false,
      });
    }
    if (m.role !== "assistant") continue;
    // Words next to tool calls ("Let me search…") stay in the thread in order; the answer,
    // the turn without calls, is the one that lists its sources.
    if (m.text) {
      const aside = m.toolCalls.length > 0;
      blocks.push({ kind: "reply", key: `reply:${m.id}`, text: m.text, read, aside });
    }
    for (const call of m.toolCalls) {
      const args = call.arguments ?? {};
      if (call.name === "get_note") {
        const note = id(args.id);
        if (note !== null) read.opened.push(note);
      }
      if (call.name === "get_project") {
        const project = text(args.project);
        if (project) read.projects.push(project);
      }
    }
    if (m.toolCalls.length > 0) {
      const last = blocks.at(-1);
      if (last?.kind === "tools") last.calls = [...last.calls, ...m.toolCalls];
      else blocks.push({ kind: "tools", key: `tools:${m.id}`, calls: m.toolCalls });
    }
    if (m.stopped) blocks.push({ kind: "stopped", key: `stopped:${m.id}` });
  }
  if (building.length > 0) {
    const key = `draft:${asked}`;
    blocks.push({ kind: "draft", key, id: 0, groups: building, status: "pending", building: true });
  }
  return blocks;
}

/** What the tools of one exchange showed the model, for finding a reply's sources. */
export type Read = {
  /** Notes any tool result listed (`note 12: …`). */
  notes: Set<number>;
  /** Notes opened with get_note, in order. */
  opened: number[];
  /** Projects opened with get_project, as the model named them. */
  projects: string[];
};

const emptyRead = (): Read => ({ notes: new Set(), opened: [], projects: [] });

const NOTE_LINE = /\bnote (\d+): /g;

export type ReplySource = { kind: "item"; id: number } | { kind: "project"; id: number };

/**
 * Where a reply's answer came from, in the order the reply first uses them: notes and overviews
 * whose secrets it shows, and notes or projects the model read whose titles it names. A reply
 * that names nothing but read a single note cites that note.
 */
export function replySources(reply: string, read: Read, tree: Tree): ReplySource[] {
  const found: { at: number; source: ReplySource }[] = [];
  const add = (at: number, source: ReplySource) => {
    const same = found.find((f) => f.source.kind === source.kind && f.source.id === source.id);
    if (same) same.at = Math.min(same.at, at);
    else found.push({ at, source });
  };
  for (const match of reply.matchAll(HANDLE)) {
    const source = handleSource(match[1], tree);
    if (source) add(match.index, source);
  }
  const lower = reply.toLowerCase();
  const named = (name: string) =>
    name.trim().length >= 3 ? lower.indexOf(name.toLowerCase()) : -1;
  for (const item of tree.items) {
    if (!read.notes.has(item.id) && !read.opened.includes(item.id)) continue;
    const at = named(item.title);
    if (at >= 0) add(at, { kind: "item", id: item.id });
  }
  const opened = new Set(read.opened);
  if (found.length === 0 && opened.size === 1) {
    const only = tree.items.find((i) => opened.has(i.id));
    if (only) add(0, { kind: "item", id: only.id });
  }
  for (const asked of read.projects) {
    const project = findProject(asked, tree);
    if (!project) continue;
    const at = named(project.name);
    const covered = found.some(
      (f) =>
        f.source.kind === "item" &&
        tree.items.find((i) => i.id === f.source.id)?.projectId === project.id,
    );
    if (at >= 0 && !covered) add(at, { kind: "project", id: project.id });
  }
  return found.sort((a, b) => a.at - b.at).map((f) => f.source);
}

/** The note or project a handle (`n12.0`, `p3.0`, `t4.0`) belongs to, if it is in the tree. */
export function handleSource(handle: string, tree: Tree): ReplySource | null {
  const id = Number(handle.slice(1).split(".")[0]);
  if (handle.startsWith("n") && tree.items.some((i) => i.id === id)) return { kind: "item", id };
  if (handle.startsWith("p") && tree.projects.some((p) => p.id === id)) {
    return { kind: "project", id };
  }
  const itemId = handle.startsWith("t") ? tree.todos.find((t) => t.id === id)?.itemId : null;
  return itemId != null ? { kind: "item", id: itemId } : null;
}

/** A project as the model named it: an id, its name, or a unique part of it. */
function findProject(asked: string, tree: Tree) {
  const byId = id(asked);
  if (byId !== null) return tree.projects.find((p) => p.id === byId);
  const lower = asked.toLowerCase();
  const exact = tree.projects.find((p) => p.name.toLowerCase() === lower);
  if (exact) return exact;
  const partial = tree.projects.filter((p) => p.name.toLowerCase().includes(lower));
  return partial.length === 1 ? partial[0] : undefined;
}
