import type { Folder, ItemSummary, ProjectSummary, Tree } from "./api";

export type FolderNode = {
  folder: Folder;
  folders: FolderNode[];
  items: ItemSummary[];
};

export type ProjectNode = {
  project: ProjectSummary;
  folders: FolderNode[];
  items: ItemSummary[];
};

export type FolderOption = { id: number | null; label: string };

const parentKey = (projectId: number, folderId: number | null) =>
  `${projectId}:${folderId ?? "root"}`;

function groupBy<T>(values: T[], key: (value: T) => string): Map<string, T[]> {
  const groups = new Map<string, T[]>();
  for (const value of values) {
    const k = key(value);
    const group = groups.get(k);
    if (group) group.push(value);
    else groups.set(k, [value]);
  }
  return groups;
}

/** Nests the flat tree from the backend into projects → folders → items. */
export function buildProjectTree(tree: Tree): ProjectNode[] {
  const foldersByParent = groupBy(tree.folders, (f) => parentKey(f.projectId, f.parentId));
  const itemsByParent = groupBy(tree.items, (i) => parentKey(i.projectId, i.folderId));

  const buildFolders = (projectId: number, parentId: number | null): FolderNode[] =>
    (foldersByParent.get(parentKey(projectId, parentId)) ?? []).map((folder) => ({
      folder,
      folders: buildFolders(projectId, folder.id),
      items: itemsByParent.get(parentKey(projectId, folder.id)) ?? [],
    }));

  return tree.projects.map((project) => ({
    project,
    folders: buildFolders(project.id, null),
    items: itemsByParent.get(parentKey(project.id, null)) ?? [],
  }));
}

/** Folders from the project root down to `folderId`, inclusive. */
export function folderPath(folders: Folder[], folderId: number | null): Folder[] {
  const byId = new Map(folders.map((f) => [f.id, f]));
  const path: Folder[] = [];
  let current = folderId === null ? undefined : byId.get(folderId);
  while (current) {
    path.unshift(current);
    current = current.parentId === null ? undefined : byId.get(current.parentId);
  }
  return path;
}

/** Root plus every folder of a project, depth-first, labelled "Parent / Child". */
export function folderOptions(tree: Tree, projectId: number): FolderOption[] {
  const project = buildProjectTree(tree).find((p) => p.project.id === projectId);
  const options: FolderOption[] = [{ id: null, label: "Project root" }];
  const visit = (nodes: FolderNode[], prefix: string) => {
    for (const node of nodes) {
      const label = prefix ? `${prefix} / ${node.folder.name}` : node.folder.name;
      options.push({ id: node.folder.id, label });
      visit(node.folders, label);
    }
  };
  visit(project?.folders ?? [], "");
  return options;
}

/** "Project / Folder / Sub" location label for an item. */
export function locationLabel(tree: Tree, item: ItemSummary): string {
  const project = tree.projects.find((p) => p.id === item.projectId)?.name ?? "";
  const folders = folderPath(tree.folders, item.folderId).map((f) => f.name);
  return [project, ...folders].join(" / ");
}
