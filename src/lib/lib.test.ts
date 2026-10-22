import { describe, expect, it } from "vitest";
import type { Folder, ItemSummary, Tree } from "./api";
import { pinKey, pinnedFirst, pinnedSet, reorder } from "./pins";
import { toggled } from "./sets";
import { sortItems } from "./sort";
import { buildTagTree, hasTag } from "./tags";
import { folderPath, locationLabel } from "./tree";

const item = (id: number, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id,
  projectId: 1,
  folderId: null,
  title: `Note ${id}`,
  secure: false,
  hasSecrets: false,
  tags: [],
  updatedAt: id,
  createdAt: id,
  ...over,
});

const folder = (id: number, parentId: number | null, name: string): Folder => ({
  id,
  projectId: 1,
  parentId,
  name,
  createdAt: id,
});

describe("tags", () => {
  const items = [
    item(1, { tags: ["env/prod", "aws"] }),
    item(2, { tags: ["env/staging"] }),
    item(3, { tags: ["env/prod"] }),
  ];

  it("nests a/b tags and counts each item once per level", () => {
    const tree = buildTagTree(items);
    const env = tree.find((n) => n.path === "env")!;
    expect(env.count).toBe(3);
    expect(env.children.map((c) => [c.path, c.count])).toEqual([
      ["env/prod", 2],
      ["env/staging", 1],
    ]);
  });

  it("matches a tag and everything below it, not siblings with the same prefix", () => {
    expect(hasTag(items[0], "env")).toBe(true);
    expect(hasTag(item(9, { tags: ["envoy"] }), "env")).toBe(false);
  });
});

describe("pins", () => {
  const tree = { pins: [{ kind: "item", id: 2 }] } as Tree;

  it("shares the tree's key format", () => {
    expect(pinKey({ kind: "folder", id: 3 })).toBe("folder:3");
    expect(pinnedSet(tree)("item", 2)).toBe(true);
    expect(pinnedSet(tree)("folder", 2)).toBe(false);
  });

  it("floats pinned entries to the top and keeps the rest in order", () => {
    expect(pinnedFirst([1, 2, 3, 4], (n) => n === 3)).toEqual([3, 1, 2, 4]);
  });

  it("reorders to the row edge nearest the pointer", () => {
    const order = ["a", "b", "c"];
    expect(reorder(order, "a", { key: "c", side: "after" })).toEqual(["b", "c", "a"]);
    expect(reorder(order, "c", { key: "a", side: "before" })).toEqual(["c", "a", "b"]);
    expect(reorder(order, "a", { key: "b", side: "before" })).toBeNull(); // already there
    expect(reorder(order, "b", { key: "b", side: "after" })).toBeNull();
  });
});

describe("sorting", () => {
  const items = [item(1, { title: "b" }), item(2, { title: "a" }), item(3, { title: "c" })];

  it("sorts by name, newest edit or newest creation, pinned first", () => {
    const none = () => false;
    expect(sortItems(items, "name", none).map((i) => i.title)).toEqual(["a", "b", "c"]);
    expect(sortItems(items, "updated", none).map((i) => i.id)).toEqual([3, 2, 1]);
    expect(sortItems(items, "name", (id) => id === 3).map((i) => i.title)).toEqual(["c", "a", "b"]);
  });
});

describe("tree", () => {
  const tree = {
    projects: [{ id: 1, name: "Acme" }],
    folders: [folder(1, null, "Infra"), folder(2, 1, "AWS")],
    items: [],
    pins: [],
    todos: [],
    payments: [],
  } as Tree;

  it("walks a folder up to the project root", () => {
    expect(folderPath(tree.folders, 2).map((f) => f.name)).toEqual(["Infra", "AWS"]);
    expect(locationLabel(tree, item(1, { folderId: 2 }))).toBe("Acme / Infra / AWS");
  });
});

describe("sets", () => {
  it("toggles membership without mutating", () => {
    const set = new Set([1]);
    expect([...toggled(set, 2)]).toEqual([1, 2]);
    expect([...toggled(set, 1)]).toEqual([]);
    expect([...set]).toEqual([1]);
  });
});
