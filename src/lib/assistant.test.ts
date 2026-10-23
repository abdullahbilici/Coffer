import { describe, expect, it } from "vitest";
import type { Message, Tree } from "./api";
import { replySources, replyStatus, threadBlocks, toolLabel, withSecretChips } from "./assistant";

const tree: Tree = {
  projects: [{ id: 1, name: "Acme" }],
  folders: [{ id: 3, projectId: 1, parentId: null, name: "Infra", createdAt: 0 }],
  items: [
    {
      id: 12,
      projectId: 1,
      folderId: 3,
      title: "Hosting panel",
      secure: false,
      hasSecrets: true,
      tags: [],
      updatedAt: 0,
      createdAt: 0,
    },
  ],
  pins: [],
  todos: [],
  payments: [],
};
tree.projects.push({ id: 2, name: "Muhit" });
tree.items.push(
  { ...tree.items[0], id: 13, title: "Redis", hasSecrets: false },
  { ...tree.items[0], id: 20, projectId: 2, folderId: null, title: ".env (staging)", secure: true },
);

describe("withSecretChips", () => {
  it("turns handles into secret markers, reusing repeated ones", () => {
    const { source, handles } = withSecretChips(
      "Password: [n12.0], key `[p3.1]`, again [n12.0], whole [n7]. Not [x1] or [n].",
    );
    expect(source).toBe("Password: `!0!`, key `!1!`, again `!0!`, whole `!2!`. Not [x1] or [n].");
    expect(handles).toEqual(["n12.0", "p3.1", "n7"]);
  });

  it("leaves text without handles alone", () => {
    expect(withSecretChips("plain `code`")).toEqual({ source: "plain `code`", handles: [] });
  });
});

describe("toolLabel", () => {
  const call = (name: string, args: Record<string, unknown> = {}) => ({
    id: "c",
    name,
    arguments: args,
  });

  it("describes calls with names from the tree", () => {
    expect(toolLabel(call("search", { query: " aws " }), tree)).toBe("Searched for aws");
    expect(toolLabel(call("get_note", { id: 12 }), tree)).toBe("Read Hosting panel");
    expect(toolLabel(call("get_note", { id: "12" }), tree)).toBe("Read Hosting panel");
    expect(toolLabel(call("get_note", { id: 99 }), tree)).toBe("Read a note");
    expect(toolLabel(call("list_folder", { folder_id: 3 }), tree)).toBe("Opened Infra");
    expect(toolLabel(call("project_spend", { project: "Acme" }), tree)).toBe(
      "Added up spend for Acme",
    );
    expect(toolLabel(call("list_todos"), tree)).toBe("Checked todos");
    expect(toolLabel(call("mystery"), tree)).toBe("mystery");
  });

  it("names drafted changes and says when one failed", () => {
    expect(toolLabel(call("create_note", { title: "Stripe" }), tree)).toBe("Drafted note Stripe");
    expect(toolLabel(call("move_note", { note: 12 }), tree)).toBe("Drafted moving Hosting panel");
    expect(toolLabel(call("add_notes", { notes: [{}, {}] }), tree)).toBe("Drafted 2 notes");
    const failed = call("update_field", { note: 12, label: "Password" });
    expect(toolLabel(failed, tree, "Error: no such field")).toBe(
      "Couldn't draft Password in Hosting panel",
    );
  });
});

describe("threadBlocks", () => {
  const call = (id: string) => ({ id, name: "search", arguments: {} });
  const at = { createdAt: 0 };

  it("groups tool calls in a row and hides tool results", () => {
    const messages: Message[] = [
      { id: 1, ...at, role: "user", text: "hi" },
      { id: 2, ...at, role: "assistant", text: "", toolCalls: [call("a")], stopped: false },
      { id: 3, ...at, role: "tool", callId: "a", name: "search", result: "r" },
      { id: 4, ...at, role: "assistant", text: "", toolCalls: [call("b")], stopped: false },
      { id: 5, ...at, role: "tool", callId: "b", name: "search", result: "r" },
      { id: 6, ...at, role: "assistant", text: "Done", toolCalls: [], stopped: false },
      { id: 7, ...at, role: "assistant", text: "", toolCalls: [], stopped: true },
    ];
    expect(threadBlocks(messages).map((b) => [b.kind, b.key])).toEqual([
      ["user", "user:1"],
      ["tools", "tools:2"],
      ["reply", "reply:6"],
      ["stopped", "stopped:7"],
    ]);
    const tools = threadBlocks(messages)[1];
    expect(tools.kind === "tools" && tools.calls.map((c) => c.id)).toEqual(["a", "b"]);
  });

  it("shows pastes, drafts and the words next to tool calls, in order", () => {
    const messages: Message[] = [
      { id: 1, ...at, role: "user", text: "", paste: "a\nb" },
      { id: 2, ...at, role: "assistant", text: "Drafted!", toolCalls: [call("a")], stopped: false },
      { id: 3, ...at, role: "tool", callId: "a", name: "create_note", result: "ok" },
      { id: 4, ...at, role: "draft", groups: [], status: "pending" },
    ];
    expect(threadBlocks(messages).map((b) => b.kind)).toEqual(["user", "reply", "tools", "draft"]);
    const aside = threadBlocks(messages)[1];
    expect(aside.kind === "reply" && aside.aside).toBe(true);
    const user = threadBlocks(messages)[0];
    expect(user.kind === "user" && user.paste).toBe("a\nb");
  });
});

describe("replySources", () => {
  const at = { createdAt: 0 };
  const call = (id: string, name: string, args: Record<string, unknown>) => ({
    id,
    name,
    arguments: args,
  });
  const exchange = (reply: string, calls: ReturnType<typeof call>[], result = ""): Message[] => [
    { id: 1, ...at, role: "user", text: "q" },
    { id: 2, ...at, role: "assistant", text: "", toolCalls: calls, stopped: false },
    ...calls.map((c, i) => ({
      id: 3 + i,
      ...at,
      role: "tool" as const,
      callId: c.id,
      name: c.name,
      result,
    })),
    { id: 9, ...at, role: "assistant", text: reply, toolCalls: [], stopped: false },
  ];
  const sources = (messages: Message[]) => {
    const block = threadBlocks(messages).find((b) => b.kind === "reply");
    if (block?.kind !== "reply") throw new Error("no reply");
    return replySources(block.text, block.read, tree);
  };

  it("cites notes whose secrets it shows and notes it read and names, in reply order", () => {
    const messages = exchange(
      "Redis is up. The panel password is [n12.0]; JWT [n20.1].",
      [call("a", "search", { query: "redis" })],
      "2 results:\n- note 13: Redis in Acme / Infra\n- note 12: Hosting panel in Acme / Infra",
    );
    expect(sources(messages)).toEqual([
      { kind: "item", id: 13 },
      { kind: "item", id: 12 },
      { kind: "item", id: 20 },
    ]);
  });

  it("names only what the model read, and falls back to the one note it opened", () => {
    expect(
      sources(exchange("Nothing about Redis here.", [call("a", "list_projects", {})])),
    ).toEqual([]);
    expect(sources(exchange("It runs nightly.", [call("a", "get_note", { id: "20" })]))).toEqual([
      { kind: "item", id: 20 },
    ]);
  });

  it("cites a project it opened and names, unless one of its notes is cited", () => {
    expect(
      sources(exchange("Muhit has no todos.", [call("a", "get_project", { project: "muhit" })])),
    ).toEqual([{ kind: "project", id: 2 }]);
    const both = exchange("Muhit's .env (staging) has it.", [
      call("a", "get_project", { project: "2" }),
      call("b", "get_note", { id: 20 }),
    ]);
    expect(sources(both)).toEqual([{ kind: "item", id: 20 }]);
  });
});

describe("replyStatus", () => {
  const user: Message = { id: 1, createdAt: 0, role: "user", text: "hi" };
  const calls = (...names: string[]): Message => ({
    id: 2,
    createdAt: 0,
    role: "assistant",
    text: "",
    stopped: false,
    toolCalls: names.map((name, i) => ({ id: `c${i}`, name, arguments: {} })),
  });
  const result = (callId: string): Message => ({
    id: 3,
    createdAt: 0,
    role: "tool",
    callId,
    name: "search",
    result: "",
  });

  it("names the phase of the model call", () => {
    expect(replyStatus([user], "loading", "")).toBe("Loading model");
    expect(replyStatus([user], "reading", "")).toBe("Reading");
    expect(replyStatus([user], "writing", "")).toBe("Writing");
    expect(replyStatus([user], "writing", "Hello")).toBeNull();
    expect(replyStatus([user], null, "")).toBeNull();
  });

  it("names the tool that is running", () => {
    expect(replyStatus([user, calls("search")], null, "")).toBe("Using search");
    expect(replyStatus([user, calls("get_note", "search"), result("c0")], null, "")).toBe(
      "Using search",
    );
    expect(replyStatus([user, calls("search"), result("c0")], "reading", "")).toBe("Reading");
    expect(replyStatus([user, calls("add_field")], null, "")).toBe("Drafting");
    expect(replyStatus([user, calls("search")], "loading", "")).toBe("Loading model");
  });
});

describe("threadBlocks while an import runs", () => {
  const user: Message = { id: 4, createdAt: 0, role: "user", text: "", paste: "dump" };
  const groups = [{ project: "Acme", new: false, entries: [] }];

  it("ends with the draft so far, keyed like the stored draft that replaces it", () => {
    const building = threadBlocks([user], groups).at(-1);
    expect(building).toMatchObject({ kind: "draft", building: true, groups });
    const draft: Message = { id: 9, createdAt: 0, role: "draft", groups, status: "pending" };
    const stored = threadBlocks([user, draft]).at(-1);
    expect(stored).toMatchObject({ kind: "draft", building: false, id: 9 });
    expect(stored?.key).toBe(building?.key);
  });

  it("shows the part instead of the phase", () => {
    expect(replyStatus([user], "reading", "", { part: 3, parts: 9, groups })).toBe("Part 3 of 9");
    expect(replyStatus([user], "loading", "", { part: 1, parts: 9, groups: [] })).toBe(
      "Loading model",
    );
  });
});
