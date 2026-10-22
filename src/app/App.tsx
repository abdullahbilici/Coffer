import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Tag } from "lucide-react";
import type { TreeEdit } from "../components/sidebar/ProjectTree";
import { SettingsModal } from "../components/settings/SettingsModal";
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
import { useAppearance } from "../hooks/useAppearance";
import { useExpansion } from "../hooks/useExpansion";
import { useNavigation } from "../hooks/useNavigation";
import { useSession } from "../hooks/useSession";
import { useToast } from "../hooks/useToast";
import { useVault } from "../hooks/useVault";
import { api, errorMessage } from "../lib/api";
import { hasTag } from "../lib/tags";
import type { Copy, Run } from "./actions";
import { useVaultActions } from "./useVaultActions";
import styles from "./App.module.css";

/** Layout and routing; state and actions live in the hooks it composes. */
export function App() {
  const session = useSession();
  const { tree, reload } = useVault(session.entered);
  const toast = useToast();
  const nav = useNavigation(tree);
  const expansion = useExpansion(tree);
  const [treeEdit, setTreeEdit] = useState<TreeEdit | null>(null);
  const [appearance, setAppearance] = useAppearance();
  const [settingsOpen, setSettingsOpen] = useState(false);
  /** Known before entering; an empty vault offers a restore on the launch screen. */
  const [vaultEmpty, setVaultEmpty] = useState(false);

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

  const copy: Copy = useCallback(
    async (text, secret = false) => {
      await navigator.clipboard.writeText(text);
      if (secret) await api.clearClipboardLater();
      showToast("Copied to clipboard", "success");
    },
    [showToast],
  );

  const { actions, saveItem, deleting } = useVaultActions({
    tree,
    reload,
    session,
    run,
    showError,
    nav,
    expansion,
    setTreeEdit,
  });

  // Ask for Touch ID once on launch (StrictMode runs effects twice in dev). An empty vault
  // waits on the launch screen instead, which offers a restore.
  const prompted = useRef(false);
  const { unlock } = session;
  useEffect(() => {
    if (prompted.current) return;
    prompted.current = true;
    void run(async () => {
      const empty = await api.vaultIsEmpty();
      setVaultEmpty(empty);
      if (!empty) await unlock();
    });
  }, [run, unlock]);

  const { select } = nav;
  const restored = useCallback(async () => {
    await reload();
    select(null);
    showToast("Vault restored", "success");
  }, [reload, select, showToast]);

  // Cmd+, opens Settings.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!event.metaKey || (event.code !== "Comma" && event.key !== ",")) return;
      event.preventDefault();
      setSettingsOpen(true);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const closeSettings = useCallback(() => setSettingsOpen(false), []);

  if (!session.entered) {
    return (
      <>
        <LockScreen
          onUnlock={() => run(session.unlock)}
          canRestore={vaultEmpty}
          run={run}
          onRestored={restored}
        />
        <Toast toast={toast.toast} onDismiss={toast.dismiss} />
      </>
    );
  }

  if (!tree) return null;

  const { selection, query } = nav;
  const shared = { tree, session, run, actions, onCopy: copy };

  const renderMain = () => {
    if (query.trim()) return <SearchView query={query} {...shared} />;

    switch (selection?.kind) {
      case "project":
        return (
          <ContainerView
            key={`project:${selection.id}`}
            projectId={selection.id}
            folderId={null}
            {...shared}
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
              {...shared}
            />
          )
        );
      }
      case "item":
        return <ItemView key={selection.id} itemId={selection.id} onSave={saveItem} {...shared} />;
      case "newItem":
        return (
          <ItemEditor
            key={`${selection.projectId}:${selection.folderId}`}
            tree={tree}
            initial={emptyDraft(selection.projectId, selection.folderId)}
            heading="New item"
            onSave={saveItem}
            onCancel={() => nav.select({ kind: "project", id: selection.projectId })}
          />
        );
      case "tag":
        return (
          <ItemListView
            icon={Tag}
            title={selection.path}
            items={tree.items.filter((i) => hasTag(i, selection.path))}
            tree={tree}
            onOpen={actions.openItem}
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
        expanded={expansion.expanded}
        onToggle={expansion.toggle}
        onExpand={expansion.expand}
        onCollapseAll={expansion.collapseAll}
        deleting={deleting}
        query={query}
        onQueryChange={nav.setQuery}
        edit={treeEdit}
        onEdit={setTreeEdit}
        session={session}
        onLock={() => run(session.lock)}
        onSettings={() => setSettingsOpen(true)}
        actions={actions}
      />
      <main className={styles.main}>
        <nav className={styles.history} aria-label="History">
          <IconButton icon={ChevronLeft} label="Back" disabled={!nav.canBack} onClick={nav.back} />
          <IconButton
            icon={ChevronRight}
            label="Forward"
            disabled={!nav.canForward}
            onClick={nav.forward}
          />
        </nav>
        {renderMain()}
      </main>
      {settingsOpen && (
        <SettingsModal
          session={session}
          run={run}
          onCopy={copy}
          appearance={appearance}
          onAppearanceChange={setAppearance}
          onRestored={restored}
          onClose={closeSettings}
        />
      )}
      <Toast toast={toast.toast} onDismiss={toast.dismiss} />
    </div>
  );
}
