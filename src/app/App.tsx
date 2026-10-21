import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Tag } from "lucide-react";
import type { TreeEdit } from "../components/sidebar/ProjectTree";
import { Sidebar } from "../components/sidebar/Sidebar";
import { IconButton } from "../components/ui/IconButton";
import { Toast } from "../components/ui/Toast";
import { ContainerView } from "../components/views/ContainerView";
import { EmptyView } from "../components/views/EmptyView";
import { ItemEditor, emptyDraft } from "../components/views/ItemEditor";
import { ItemListView } from "../components/views/ItemListView";
import { ItemView } from "../components/views/ItemView";
import { LockScreen } from "../components/views/LockScreen";
import { SearchView } from "../components/views/SearchView";
import { useHistory, type Place } from "../hooks/useHistory";
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
      selectRef.current(null);
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

  // Back/forward history. Going back to a search restores its results.
  const treeRef = useRef(tree);
  treeRef.current = tree;
  const placeExists = useCallback((place: Place) => {
    const t = treeRef.current;
    const s = place.selection;
    if (!t || !s) return true;
    switch (s.kind) {
      case "project":
        return t.projects.some((p) => p.id === s.id);
      case "folder":
        return t.folders.some((f) => f.id === s.id);
      case "item":
        return t.items.some((i) => i.id === s.id);
      case "tag":
        return t.items.some((i) => hasTag(i, s.path));
      case "newItem":
        return false;
    }
  }, []);
  const history = useHistory((place) => {
    setSelection(place.selection);
    setQuery(place.query);
  }, placeExists);
  const { record } = history;

  const select = useCallback(
    (next: Selection) => {
      record({ selection, query }, { selection: next, query: "" });
      setSelection(next);
      setQuery("");
    },
    [record, selection, query],
  );
  const selectRef = useRef(select);
  selectRef.current = select;

  const here: Place = { selection, query };
  const goBack = () => history.back(here);
  const goForward = () => history.forward(here);
  const navRef = useRef({ goBack, goForward });
  navRef.current = { goBack, goForward };

  // Cmd+[ / Cmd+] by physical key (Cmd+Ğ / Cmd+Ü on a Turkish keyboard), Cmd+arrows outside
  // text fields, and the mouse's back/forward buttons.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!event.metaKey || event.altKey || event.ctrlKey || event.shiftKey) return;
      const editing = (event.target as HTMLElement).closest(
        "input, textarea, select, [contenteditable]",
      );
      if (editing) return;
      if (event.code === "BracketLeft" || event.key === "ArrowLeft") {
        event.preventDefault();
        navRef.current.goBack();
      } else if (event.code === "BracketRight" || event.key === "ArrowRight") {
        event.preventDefault();
        navRef.current.goForward();
      }
    };
    const onMouseUp = (event: MouseEvent) => {
      if (event.button === 3) navRef.current.goBack();
      if (event.button === 4) navRef.current.goForward();
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("mouseup", onMouseUp);
    };
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
      guard: (then) =>
        run(async () => {
          await session.ensureUnlocked();
          then();
        }),
      newItem: (projectId, folderId) =>
        run(async () => {
          await session.ensureUnlocked();
          select({ kind: "newItem", projectId, folderId });
        }),

      createProject: (name) =>
        run(async () => {
          const project = await session.withUnlock(() => api.createProject(name));
          await reload();
          select({ kind: "project", id: project.id });
        }),
      renameProject: (id, name) =>
        run(async () => {
          await session.withUnlock(() => api.renameProject(id, name));
          await reload();
        }),
      deleteProject: (id) =>
        run(async () => {
          await session.withUnlock(() => api.deleteProject(id));
          await reload();
          setSelection(null);
        }),

      startCreateFolder: (projectId, parentId) =>
        run(async () => {
          await session.ensureUnlocked();
          expandTo(projectId, parentId);
          setTreeEdit({ type: "createFolder", projectId, parentId });
        }),
      createFolder: (projectId, parentId, name) =>
        run(async () => {
          await session.withUnlock(() => api.createFolder(projectId, parentId, name));
          await reload();
        }),
      renameFolder: (id, name) =>
        run(async () => {
          await session.withUnlock(() => api.renameFolder(id, name));
          await reload();
        }),
      // After a move, open the destination so the moved row is visible where it landed.
      moveFolder: (id, projectId, parentId) =>
        run(async () => {
          await session.withUnlock(() => api.moveFolder(id, projectId, parentId));
          expandTo(projectId, parentId);
          await reload();
        }),
      moveItem: (id, projectId, folderId) =>
        run(async () => {
          await session.withUnlock(() => api.moveItem(id, projectId, folderId));
          expandTo(projectId, folderId);
          await reload();
        }),
      deleteFolder: (id) =>
        run(async () => {
          await session.withUnlock(() => api.deleteFolder(id));
          await reload();
          setSelection(null);
        }),

      duplicateItem: (id) =>
        run(async () => {
          const copy = await session.withUnlock(() => api.duplicateItem(id));
          await reload();
          openItem(copy);
        }),
      deleteItem: (id) =>
        run(async () => {
          await session.withUnlock(() => api.deleteItem(id));
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
    if (query.trim()) {
      return (
        <SearchView
          query={query}
          tree={tree}
          session={session}
          run={run}
          actions={actions}
          onCopy={copy}
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
      <main className={styles.main}>
        <nav className={styles.history} aria-label="History">
          <IconButton
            icon={ChevronLeft}
            label="Back"
            disabled={!history.canBack}
            onClick={goBack}
          />
          <IconButton
            icon={ChevronRight}
            label="Forward"
            disabled={!history.canForward}
            onClick={goForward}
          />
        </nav>
        {renderMain()}
      </main>
      <Toast toast={toast.toast} onDismiss={toast.dismiss} />
    </div>
  );
}
