import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Search, Tag } from "lucide-react";
import type { TreeEdit } from "../components/sidebar/ProjectTree";
import { Sidebar } from "../components/sidebar/Sidebar";
import { Toast } from "../components/ui/Toast";
import { ContainerView } from "../components/views/ContainerView";
import { EmptyView } from "../components/views/EmptyView";
import { ItemEditor, emptyDraft } from "../components/views/ItemEditor";
import { ItemListView } from "../components/views/ItemListView";
import { ItemView } from "../components/views/ItemView";
import { LockScreen } from "../components/views/LockScreen";
import { useSession } from "../hooks/useSession";
import { useToast } from "../hooks/useToast";
import { useVault } from "../hooks/useVault";
import { api, errorMessage, type ItemSummary, type SaveItemInput } from "../lib/api";
import { inSection, nodeKey, type Selection, type TreeSection } from "../lib/selection";
import { hasTag } from "../lib/tags";
import { folderPath } from "../lib/tree";
import type { Run, VaultActions } from "./actions";
import styles from "./App.module.css";

export function App() {
  const session = useSession();
  const { tree, reload } = useVault(session.entered);
  const toast = useToast();
  const [selection, setSelection] = useState<Selection>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const [query, setQuery] = useState("");
  const [treeEdit, setTreeEdit] = useState<TreeEdit | null>(null);

  const { show: showToast } = toast;
  const showError = useCallback((error: unknown) => showToast(errorMessage(error)), [showToast]);
  const run: Run = useCallback(
    async (action) => {
      try {
        await action();
      } catch (error) {
        showError(error);
      }
    },
    [showError],
  );

  const copy = useCallback(
    async (text: string) => {
      await navigator.clipboard.writeText(text);
      showToast("Copied to clipboard", "success");
    },
    [showToast],
  );

  // Esc clears the main pane, unless a field, menu or editor already handled it.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      const target = event.target as HTMLElement;
      if (target.closest("input, textarea, select, [contenteditable]")) return;
      setSelection(null);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  // Ask for Touch ID once on launch (StrictMode runs effects twice in dev).
  const prompted = useRef(false);
  useEffect(() => {
    if (prompted.current) return;
    prompted.current = true;
    void run(session.unlock);
  }, [run, session.unlock]);

  const toggle = useCallback((key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  }, []);

  const collapseAll = useCallback((section: TreeSection) => {
    setExpanded((prev) => new Set([...prev].filter((key) => !inSection(key, section))));
  }, []);

  const expand = useCallback((keys: string[]) => {
    setExpanded((prev) => (keys.every((k) => prev.has(k)) ? prev : new Set([...prev, ...keys])));
  }, []);

  const select = useCallback((next: Selection) => {
    setSelection(next);
    setQuery("");
  }, []);

  /** Expands a project and the folders down to `folderId` so that location is visible. */
  const expandTo = useCallback(
    (projectId: number, folderId: number | null) => {
      const folders = tree ? folderPath(tree.folders, folderId) : [];
      expand([nodeKey.project(projectId), ...folders.map((f) => nodeKey.folder(f.id))]);
    },
    [tree, expand],
  );

  const openItem = useCallback(
    (item: ItemSummary) => {
      expandTo(item.projectId, item.folderId);
      select({ kind: "item", id: item.id });
    },
    [expandTo, select],
  );

  /** Rejects on failure (after showing a toast) so the editor stays open with the draft. */
  const saveItem = useCallback(
    async (input: SaveItemInput) => {
      try {
        const saved = await session.withUnlock(() => api.saveItem(input));
        await reload();
        openItem(saved);
      } catch (error) {
        showError(error);
        throw error;
      }
    },
    [session, reload, openItem, showError],
  );

  const actions: VaultActions = useMemo(
    () => ({
      selectProject: (id) => select({ kind: "project", id }),
      selectFolder: (id) => {
        const folder = tree?.folders.find((f) => f.id === id);
        if (folder) expandTo(folder.projectId, folder.id);
        select({ kind: "folder", id });
      },
      selectTag: (path) => select({ kind: "tag", path }),
      openItem,
      newItem: (projectId, folderId) => select({ kind: "newItem", projectId, folderId }),

      createProject: (name) =>
        run(async () => {
          const project = await api.createProject(name);
          await reload();
          select({ kind: "project", id: project.id });
        }),
      renameProject: (id, name) =>
        run(async () => {
          await api.renameProject(id, name);
          await reload();
        }),
      deleteProject: (id) =>
        run(async () => {
          await api.deleteProject(id);
          await reload();
          setSelection(null);
        }),

      startCreateFolder: (projectId, parentId) => {
        expandTo(projectId, parentId);
        setTreeEdit({ type: "createFolder", projectId, parentId });
      },
      createFolder: (projectId, parentId, name) =>
        run(async () => {
          await api.createFolder(projectId, parentId, name);
          await reload();
        }),
      renameFolder: (id, name) =>
        run(async () => {
          await api.renameFolder(id, name);
          await reload();
        }),
      deleteFolder: (id) =>
        run(async () => {
          await api.deleteFolder(id);
          await reload();
          setSelection(null);
        }),

      duplicateItem: (id) =>
        run(async () => {
          const copy = await api.duplicateItem(id);
          await reload();
          openItem(copy);
        }),
      deleteItem: (id) =>
        run(async () => {
          await api.deleteItem(id);
          await reload();
          setSelection((s) => (s?.kind === "item" && s.id === id ? null : s));
        }),
    }),
    [run, reload, select, openItem, expandTo, tree, session],
  );

  if (!session.entered) {
    return (
      <>
        <LockScreen onUnlock={() => run(session.unlock)} />
        <Toast toast={toast.toast} onDismiss={toast.dismiss} />
      </>
    );
  }

  if (!tree) return null;

  const renderMain = () => {
    const q = query.trim().toLowerCase();
    if (q) {
      const results = tree.items.filter(
        (i) => i.title.toLowerCase().includes(q) || i.tags.some((t) => t.toLowerCase().includes(q)),
      );
      return (
        <ItemListView
          icon={Search}
          title={`Results for "${query.trim()}"`}
          items={results}
          tree={tree}
          onOpen={openItem}
          onSelectTag={actions.selectTag}
        />
      );
    }

    switch (selection?.kind) {
      case "project":
        return (
          <ContainerView
            key={`project:${selection.id}`}
            projectId={selection.id}
            folderId={null}
            tree={tree}
            run={run}
            actions={actions}
            session={session}
            onCopy={copy}
          />
        );
      case "folder": {
        const folder = tree.folders.find((f) => f.id === selection.id);
        return (
          folder && (
            <ContainerView
              key={`folder:${folder.id}`}
              projectId={folder.projectId}
              folderId={folder.id}
              tree={tree}
              run={run}
              actions={actions}
              session={session}
              onCopy={copy}
            />
          )
        );
      }
      case "item":
        return (
          <ItemView
            key={selection.id}
            itemId={selection.id}
            tree={tree}
            session={session}
            run={run}
            actions={actions}
            onSave={saveItem}
            onCopy={copy}
          />
        );
      case "newItem":
        return (
          <ItemEditor
            key={`${selection.projectId}:${selection.folderId}`}
            tree={tree}
            initial={emptyDraft(selection.projectId, selection.folderId)}
            heading="New item"
            onSave={saveItem}
            onCancel={() => select({ kind: "project", id: selection.projectId })}
          />
        );
      case "tag":
        return (
          <ItemListView
            icon={Tag}
            title={selection.path}
            items={tree.items.filter((i) => hasTag(i, selection.path))}
            tree={tree}
            onOpen={openItem}
            onSelectTag={actions.selectTag}
          />
        );
      default:
        return <EmptyView />;
    }
  };

  return (
    <div className={styles.layout}>
      <Sidebar
        tree={tree}
        selection={selection}
        expanded={expanded}
        onToggle={toggle}
        onExpand={expand}
        onCollapseAll={collapseAll}
        query={query}
        onQueryChange={setQuery}
        edit={treeEdit}
        onEdit={setTreeEdit}
        session={session}
        onLock={() => run(session.lock)}
        actions={actions}
      />
      <main className={styles.main}>{renderMain()}</main>
      <Toast toast={toast.toast} onDismiss={toast.dismiss} />
    </div>
  );
}
