import { describe, expect, it } from "vitest";
import type { Tree } from "./api";
import { toolLabel, withSecretChips } from "./assistant";

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
});
