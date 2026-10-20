import { useState, type MouseEvent } from "react";
import {
  Box,
  CopyPlus,
  Ellipsis,
  FilePlus,
  Folder,
  FolderOpen,
  FolderPlus,
  Pencil,
  Trash2,
} from "lucide-react";
import type { VaultActions } from "../../app/actions";
import type { ItemSummary } from "../../lib/api";
import { nodeKey, type Selection } from "../../lib/selection";
import type { FolderNode, ProjectNode } from "../../lib/tree";
import { itemIcon } from "../itemIcon";
import { IconButton } from "../ui/IconButton";
import { Menu, type MenuAnchor, type MenuItem } from "../ui/Menu";
import { TreeEditRow, TreeRow } from "./TreeRow";
import styles from "./Sidebar.module.css";

/** Inline create/rename field currently open in the tree. */
export type TreeEdit =
  | { type: "createProject" }
  | { type: "createFolder"; projectId: number; parentId: number | null }
  | { type: "renameProject"; id: number }
  | { type: "renameFolder"; id: number };

type Props = {
  nodes: ProjectNode[];
  expanded: ReadonlySet<string>;
  selection: Selection;
  edit: TreeEdit | null;
  onEdit: (edit: TreeEdit | null) => void;
  onToggle: (key: string) => void;
  onExpand: (keys: string[]) => void;
  actions: VaultActions;
};

type OpenMenu = (event: MouseEvent, items: MenuItem[]) => void;

type Context = Props & { openMenu: OpenMenu };

/** First click selects and opens a node; clicking the selected node again toggles it. */
function clickNode(ctx: Context, key: string, selected: boolean, select: () => void) {
  if (selected) {
    ctx.onToggle(key);
  } else {
    select();
    ctx.onExpand([key]);
  }
}

export function ProjectTree(props: Props) {
  const [menu, setMenu] = useState<{ anchor: MenuAnchor; items: MenuItem[] } | null>(null);

  // Right-click opens at the cursor; the "more" button opens below itself.
  const openMenu: OpenMenu = (event, items) => {
    event.preventDefault();
    event.stopPropagation();
    const anchor =
      event.type === "contextmenu"
        ? { x: event.clientX, y: event.clientY }
        : (() => {
            const rect = event.currentTarget.getBoundingClientRect();
            return { x: rect.left, y: rect.bottom + 4 };
          })();
    setMenu({ anchor, items });
  };

  const ctx: Context = { ...props, openMenu };
  const { nodes, edit, onEdit, actions } = props;

  return (
    <div role="tree">
      {edit?.type === "createProject" && (
        <TreeEditRow
          depth={0}
          icon={Box}
          placeholder="Project name"
          onSubmit={(name) => {
            onEdit(null);
            actions.createProject(name);
          }}
          onCancel={() => onEdit(null)}
        />
      )}
      {nodes.map((node) => (
        <ProjectBranch key={node.project.id} node={node} ctx={ctx} />
      ))}
      {nodes.length === 0 && edit?.type !== "createProject" && (
        <p className={styles.empty}>No projects</p>
      )}
      {menu && <Menu anchor={menu.anchor} items={menu.items} onClose={() => setMenu(null)} />}
    </div>
  );
}

function ProjectBranch({ node, ctx }: { node: ProjectNode; ctx: Context }) {
  const { project } = node;
  const { expanded, selection, edit, onEdit, onToggle, onExpand, actions, openMenu } = ctx;
  const key = nodeKey.project(project.id);
  const isOpen = expanded.has(key);
  const selected = selection?.kind === "project" && selection.id === project.id;

  const newFolder = () => actions.startCreateFolder(project.id, null);
  const newItem = () => {
    onExpand([key]);
    actions.newItem(project.id, null);
  };
  const menuItems: MenuItem[] = [
    { label: "New item", icon: FilePlus, onSelect: newItem },
    { label: "New folder", icon: FolderPlus, onSelect: newFolder },
    {
      label: "Rename",
      icon: Pencil,
      onSelect: () => onEdit({ type: "renameProject", id: project.id }),
    },
    {
      label: "Delete project",
      icon: Trash2,
      danger: true,
      confirmLabel: "Delete with all items",
      onSelect: () => actions.deleteProject(project.id),
    },
  ];

  return (
    <>
      {edit?.type === "renameProject" && edit.id === project.id ? (
        <TreeEditRow
          depth={0}
          icon={Box}
          initialValue={project.name}
          placeholder="Project name"
          onSubmit={(name) => {
            onEdit(null);
            actions.renameProject(project.id, name);
          }}
          onCancel={() => onEdit(null)}
        />
      ) : (
        <TreeRow
          depth={0}
          icon={Box}
          label={project.name}
          expanded={isOpen}
          selected={selected}
          onClick={() => clickNode(ctx, key, selected, () => actions.selectProject(project.id))}
          onToggle={() => onToggle(key)}
          onContextMenu={(e) => openMenu(e, menuItems)}
          actions={
            <>
              <IconButton icon={FilePlus} label="New item" size="sm" onClick={newItem} />
              <IconButton icon={FolderPlus} label="New folder" size="sm" onClick={newFolder} />
              <IconButton
                icon={Ellipsis}
                label="More"
                size="sm"
                onClick={(e) => openMenu(e, menuItems)}
              />
            </>
          }
        />
      )}
      {isOpen && (
        <Children
          folders={node.folders}
          items={node.items}
          projectId={project.id}
          parentId={null}
          depth={1}
          ctx={ctx}
        />
      )}
    </>
  );
}

type ChildrenProps = {
  folders: FolderNode[];
  items: ItemSummary[];
  projectId: number;
  parentId: number | null;
  depth: number;
  ctx: Context;
};

/** Items first, then folders. */
function Children({ folders, items, projectId, parentId, depth, ctx }: ChildrenProps) {
  const { edit, onEdit, actions } = ctx;
  const creatingHere =
    edit?.type === "createFolder" && edit.projectId === projectId && edit.parentId === parentId;
  const empty = folders.length === 0 && items.length === 0 && !creatingHere;

  return (
    <>
      {items.map((item) => (
        <ItemRow key={item.id} item={item} depth={depth} ctx={ctx} />
      ))}
      {creatingHere && (
        <TreeEditRow
          depth={depth}
          icon={Folder}
          placeholder="Folder name"
          onSubmit={(name) => {
            onEdit(null);
            actions.createFolder(projectId, parentId, name);
          }}
          onCancel={() => onEdit(null)}
        />
      )}
      {folders.map((node) => (
        <FolderBranch key={node.folder.id} node={node} depth={depth} ctx={ctx} />
      ))}
      {empty && (
        <p className={styles.emptyNested} style={{ paddingLeft: 30 + depth * 14 }}>
          Empty
        </p>
      )}
    </>
  );
}

function FolderBranch({ node, depth, ctx }: { node: FolderNode; depth: number; ctx: Context }) {
  const { folder } = node;
  const { expanded, selection, edit, onEdit, onToggle, onExpand, actions, openMenu } = ctx;
  const key = nodeKey.folder(folder.id);
  const isOpen = expanded.has(key);
  const selected = selection?.kind === "folder" && selection.id === folder.id;

  const newFolder = () => actions.startCreateFolder(folder.projectId, folder.id);
  const newItem = () => {
    onExpand([key]);
    actions.newItem(folder.projectId, folder.id);
  };
  const menuItems: MenuItem[] = [
    { label: "New item", icon: FilePlus, onSelect: newItem },
    { label: "New folder", icon: FolderPlus, onSelect: newFolder },
    {
      label: "Rename",
      icon: Pencil,
      onSelect: () => onEdit({ type: "renameFolder", id: folder.id }),
    },
    {
      label: "Delete folder",
      icon: Trash2,
      danger: true,
      confirmLabel: "Delete with all items",
      onSelect: () => actions.deleteFolder(folder.id),
    },
  ];

  return (
    <>
      {edit?.type === "renameFolder" && edit.id === folder.id ? (
        <TreeEditRow
          depth={depth}
          icon={Folder}
          initialValue={folder.name}
          placeholder="Folder name"
          onSubmit={(name) => {
            onEdit(null);
            actions.renameFolder(folder.id, name);
          }}
          onCancel={() => onEdit(null)}
        />
      ) : (
        <TreeRow
          depth={depth}
          icon={isOpen ? FolderOpen : Folder}
          label={folder.name}
          expanded={isOpen}
          selected={selected}
          onClick={() => clickNode(ctx, key, selected, () => actions.selectFolder(folder.id))}
          onToggle={() => onToggle(key)}
          onContextMenu={(e) => openMenu(e, menuItems)}
          actions={
            <>
              <IconButton icon={FilePlus} label="New item" size="sm" onClick={newItem} />
              <IconButton icon={FolderPlus} label="New folder" size="sm" onClick={newFolder} />
              <IconButton
                icon={Ellipsis}
                label="More"
                size="sm"
                onClick={(e) => openMenu(e, menuItems)}
              />
            </>
          }
        />
      )}
      {isOpen && (
        <Children
          folders={node.folders}
          items={node.items}
          projectId={folder.projectId}
          parentId={folder.id}
          depth={depth + 1}
          ctx={ctx}
        />
      )}
    </>
  );
}

function ItemRow({ item, depth, ctx }: { item: ItemSummary; depth: number; ctx: Context }) {
  const { selection, actions, openMenu } = ctx;
  const menuItems: MenuItem[] = [
    { label: "Duplicate", icon: CopyPlus, onSelect: () => actions.duplicateItem(item.id) },
    {
      label: "Delete item",
      icon: Trash2,
      danger: true,
      confirmLabel: "Click again to delete",
      onSelect: () => actions.deleteItem(item.id),
    },
  ];
  return (
    <TreeRow
      depth={depth}
      icon={itemIcon(item)}
      label={item.title}
      selected={selection?.kind === "item" && selection.id === item.id}
      onClick={() => actions.openItem(item)}
      onContextMenu={(e) => openMenu(e, menuItems)}
      actions={
        <IconButton
          icon={Ellipsis}
          label="More"
          size="sm"
          onClick={(e) => openMenu(e, menuItems)}
        />
      }
    />
  );
}
