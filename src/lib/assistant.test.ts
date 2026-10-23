import { describe, expect, it } from "vitest";
import type { Message, Tree } from "./api";
import { threadBlocks, toolLabel, withSecretChips } from "./assistant";

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

  it("shows pastes and drafts, and only answers without tool calls", () => {
    const messages: Message[] = [
      { id: 1, ...at, role: "user", text: "", paste: "a\nb" },
      { id: 2, ...at, role: "assistant", text: "Drafted!", toolCalls: [call("a")], stopped: false },
      { id: 3, ...at, role: "tool", callId: "a", name: "create_note", result: "ok" },
      { id: 4, ...at, role: "draft", groups: [], status: "pending" },
    ];
    expect(threadBlocks(messages).map((b) => b.kind)).toEqual(["user", "tools", "draft"]);
    const user = threadBlocks(messages)[0];
    expect(user.kind === "user" && user.paste).toBe("a\nb");
  });
});
