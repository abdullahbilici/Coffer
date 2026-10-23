import type { Message, ToolCall, Tree } from "./api";

/**
 * A secret handle the assistant writes: `[n12.0]` (note 12, secret 0), `[p3.0]` (project
 * overview), `[t4.0]` (todo), `[n12]` (a whole secure note). Mirrors `assistant/handles.rs`.
 * Models sometimes wrap it in backticks; those go too.
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
export function toolLabel(call: ToolCall, tree: Tree): string {
  const args = call.arguments ?? {};
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
    default:
      return call.name;
  }
}

/** What the thread shows, in order. Tool calls in a row form one group, whichever turns made them. */
export type Block =
  | { kind: "user"; key: string; text: string }
  | { kind: "reply"; key: string; text: string }
  | { kind: "tools"; key: string; calls: ToolCall[] }
  | { kind: "stopped"; key: string };

export function threadBlocks(messages: Message[]): Block[] {
  const blocks: Block[] = [];
  for (const m of messages) {
    if (m.role === "user") blocks.push({ kind: "user", key: `user:${m.id}`, text: m.text });
    if (m.role !== "assistant") continue;
    if (m.text) blocks.push({ kind: "reply", key: `reply:${m.id}`, text: m.text });
    if (m.toolCalls.length > 0) {
      const last = blocks.at(-1);
      if (last?.kind === "tools") last.calls = [...last.calls, ...m.toolCalls];
      else blocks.push({ kind: "tools", key: `tools:${m.id}`, calls: m.toolCalls });
    }
    if (m.stopped) blocks.push({ kind: "stopped", key: `stopped:${m.id}` });
  }
  return blocks;
}
