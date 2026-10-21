import { useState, type MouseEvent } from "react";
import {
  Box,
  CopyPlus,
  Pin,
  PinOff,
  Ellipsis,
  FilePlus,
  Folder,
  FolderOpen,
  FolderPlus,
  Pencil,
  Trash2,
} from "lucide-react";
import type { VaultActions } from "../../app/actions";
import type { ItemSummary, PinKind } from "../../lib/api";
import { pinnedFirst } from "../../lib/pins";
import { nodeKey, type Selection } from "../../lib/selection";
import type { FolderNode, ProjectNode } from "../../lib/tree";
import { itemIcon } from "../itemIcon";
import { IconButton } from "../ui/IconButton";
import { CONFIRM_DELETE, Menu, type MenuAnchor, type MenuItem } from "../ui/Menu";
import { PinBadge } from "../ui/PinBadge";
import { Collapse } from "../ui/Collapse";
import { sameTarget, useTreeDnd, useTreeNode } from "./TreeDnd";
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
  isPinned: (kind: PinKind, id: number) => boolean;
  deleting: ReadonlySet<string>;
  actions: VaultActions;
};

type OpenMenu = (event: MouseEvent, items: MenuItem[]) => void;

type Context = Props & { openMenu: OpenMenu };

/** Pin / Unpin menu entry for a row. */
function pinMenuItem(ctx: Context, kind: PinKind, id: number): MenuItem {
  const pinned = ctx.isPinned(kind, id);
  return {
    label: pinned ? "Unpin" : "Pin",
    icon: pinned ? PinOff : Pin,
    onSelect: () => ctx.actions.setPinned({ kind, id }, !pinned),
  };
}

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
      {pinnedFirst(nodes, (n) => props.isPinned("project", n.project.id)).map((node) => (
        <ContainerBranch key={node.project.id} spec={projectSpec(node)} ctx={ctx} />
      ))}
      {nodes.length === 0 && edit?.type !== "createProject" && (
        <p className={styles.empty}>No projects</p>
      )}
      {menu && <Menu anchor={menu.anchor} items={menu.items} onClose={() => setMenu(null)} />}
    </div>
  );
}

type BranchSpec = {
  kind: "project" | "folder";
  id: number;
  name: string;
  projectId: number;
  /** This container's folder id; `null` for a project root. */
  folderId: number | null;
  /** For folders: the folder it currently lives in, so dropping back there is a no-op. */
  parentId: number | null;
  folders: FolderNode[];
  items: ItemSummary[];
  depth: number;
};

const projectSpec = (node: ProjectNode): BranchSpec => ({
  kind: "project",
  id: node.project.id,
  name: node.project.name,
  projectId: node.project.id,
  folderId: null,
  parentId: null,
  folders: node.folders,
  items: node.items,
  depth: 0,
});

const folderSpec = (node: FolderNode, depth: number): BranchSpec => ({
  kind: "folder",
  id: node.folder.id,
  name: node.folder.name,
  projectId: node.folder.projectId,
  folderId: node.folder.id,
  parentId: node.folder.parentId,
  folders: node.folders,
  items: node.items,
  depth,
});

/** A project or folder row with its children; both behave the same except where noted. */
function ContainerBranch({ spec, ctx }: { spec: BranchSpec; ctx: Context }) {
  const { kind, id, name, projectId, folderId, depth } = spec;
  const { expanded, selection, edit, onEdit, onToggle, onExpand, actions, openMenu } = ctx;
  const isProject = kind === "project";
  const key = isProject ? nodeKey.project(id) : nodeKey.folder(id);
  const isOpen = expanded.has(key);
  const selected = selection?.kind === kind && selection.id === id;
  const here = { projectId, folderId };
  const dnd = useTreeNode({
    id: key,
    // Projects are drop targets only; folders can also be dragged.
    subject: isProject
      ? undefined
      : {
          kind: "folder",
          id,
          label: name,
          icon: Folder,
          from: { projectId, folderId: spec.parentId },
        },
    drop: { target: here, label: name, rowId: key, expandKey: key },
  });
  const dropTarget = sameTarget(useTreeDnd().target, here);

  const renameEdit: TreeEdit = isProject
    ? { type: "renameProject", id }
    : { type: "renameFolder", id };
  const renaming = edit?.type === renameEdit.type && "id" in edit && edit.id === id;
  const icon = isProject ? Box : isOpen ? FolderOpen : Folder;

  const newFolder = () => actions.startCreateFolder(projectId, folderId);
  const newItem = () => {
    onExpand([key]);
    actions.newItem(projectId, folderId);
  };
  const menuItems: MenuItem[] = [
    pinMenuItem(ctx, kind, id),
    { label: "New item", icon: FilePlus, onSelect: newItem },
    { label: "New folder", icon: FolderPlus, onSelect: newFolder },
    { label: "Rename", icon: Pencil, onSelect: () => actions.guard(() => onEdit(renameEdit)) },
    {
      label: isProject ? "Delete project" : "Delete folder",
      icon: Trash2,
      danger: true,
      confirmLabel: CONFIRM_DELETE,
      onSelect: () => (isProject ? actions.deleteProject(id) : actions.deleteFolder(id)),
    },
  ];

  return (
    <Collapse leaving={ctx.deleting.has(key)}>
      {renaming ? (
        <TreeEditRow
          depth={depth}
          icon={isProject ? Box : Folder}
          initialValue={name}
          placeholder={isProject ? "Project name" : "Folder name"}
          onSubmit={(next) => {
            onEdit(null);
            if (isProject) actions.renameProject(id, next);
            else actions.renameFolder(id, next);
          }}
          onCancel={() => onEdit(null)}
        />
      ) : (
        <TreeRow
          depth={depth}
          icon={icon}
          label={name}
          trailing={ctx.isPinned(kind, id) ? <PinBadge /> : undefined}
          expanded={isOpen}
          selected={selected}
          dnd={dnd}
          dropTarget={dropTarget}
          onClick={() =>
            clickNode(ctx, key, selected, () =>
              isProject ? actions.selectProject(id) : actions.selectFolder(id),
            )
          }
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
          folders={spec.folders}
          items={spec.items}
          projectId={projectId}
          parentId={folderId}
          container={{ rowId: key, label: name }}
          depth={depth + 1}
          ctx={ctx}
        />
      )}
    </Collapse>
  );
}

type ChildrenProps = {
  folders: FolderNode[];
  items: ItemSummary[];
  projectId: number;
  parentId: number | null;
  /** The project or folder row these children belong to; drops on an item land there. */
  container: Container;
  depth: number;
  ctx: Context;
};

type Container = { rowId: string; label: string };

/** Folders first (a new folder's name field on top), then items. */
function Children({ folders, items, projectId, parentId, container, depth, ctx }: ChildrenProps) {
  const { edit, onEdit, actions } = ctx;
  const creatingHere =
    edit?.type === "createFolder" && edit.projectId === projectId && edit.parentId === parentId;
  const empty = folders.length === 0 && items.length === 0 && !creatingHere;

  return (
    <>
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
      {pinnedFirst(folders, (n) => ctx.isPinned("folder", n.folder.id)).map((node) => (
        <ContainerBranch key={node.folder.id} spec={folderSpec(node, depth)} ctx={ctx} />
      ))}
      {pinnedFirst(items, (i) => ctx.isPinned("item", i.id)).map((item) => (
        <ItemRow key={item.id} item={item} container={container} depth={depth} ctx={ctx} />
      ))}
      {empty && (
        <p className={styles.emptyNested} style={{ paddingLeft: 30 + depth * 14 }}>
          Empty
        </p>
      )}
    </>
  );
}

type ItemRowProps = { item: ItemSummary; container: Container; depth: number; ctx: Context };

function ItemRow({ item, container, depth, ctx }: ItemRowProps) {
  const { selection, actions, openMenu } = ctx;
  const parent = { projectId: item.projectId, folderId: item.folderId };
  // Dropping onto an item lands in the item's folder, so the whole open folder is a target.
  const dnd = useTreeNode({
    id: nodeKey.item(item.id),
    subject: { kind: "item", id: item.id, label: item.title, icon: itemIcon(item), from: parent },
    drop: { target: parent, ...container },
  });
  const menuItems: MenuItem[] = [
    pinMenuItem(ctx, "item", item.id),
    { label: "Duplicate", icon: CopyPlus, onSelect: () => actions.duplicateItem(item.id) },
    {
      label: "Delete item",
      icon: Trash2,
      danger: true,
      confirmLabel: CONFIRM_DELETE,
      onSelect: () => actions.deleteItem(item.id),
    },
  ];
  return (
    <Collapse leaving={ctx.deleting.has(nodeKey.item(item.id))}>
      <TreeRow
        depth={depth}
        icon={itemIcon(item)}
        label={item.title}
        trailing={ctx.isPinned("item", item.id) ? <PinBadge /> : undefined}
        dnd={dnd}
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
    </Collapse>
  );
}
