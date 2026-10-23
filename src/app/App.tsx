import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
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
import { PaymentEditor } from "../components/views/PaymentEditor";
import { PaymentsView } from "../components/views/PaymentsView";
import { SearchView } from "../components/views/SearchView";
import { TodoEditor } from "../components/views/TodoEditor";
import { TodosView } from "../components/views/TodosView";
import { AssistantView } from "../components/views/AssistantView";
import { useAppearance } from "../hooks/useAppearance";
import { useAssistant } from "../hooks/useAssistant";
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

/** Mirrors `capture::OPEN`: quick capture hands a conversation over to this window. */
const CAPTURE_OPEN = "capture-open";

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
      if (secret) await api.copySecret(text);
      else await navigator.clipboard.writeText(text);
      showToast("Copied to clipboard", "success");
    },
    [showToast],
  );

  const assistant = useAssistant(session, run, reload);
  const composerRef = useRef<HTMLTextAreaElement>(null);

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

  // Cmd+, opens Settings; Cmd+J opens the assistant with its field focused.
  const { selectAssistant } = actions;
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!event.metaKey) return;
      if (event.code === "Comma" || event.key === ",") {
        event.preventDefault();
        setSettingsOpen(true);
      } else if (event.code === "KeyJ" || event.key.toLowerCase() === "j") {
        event.preventDefault();
        selectAssistant();
        requestAnimationFrame(() => composerRef.current?.focus());
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selectAssistant]);

  // Registered once; reads the latest handlers through a ref.
  const openFromCapture = useRef<(id: number) => void>(() => {});
  openFromCapture.current = (id) => {
    setSettingsOpen(false);
    selectAssistant();
    void assistant.load();
    void assistant.open(id);
  };
  useEffect(() => {
    const unlisten = listen<number>(CAPTURE_OPEN, (e) => openFromCapture.current(e.payload));
    return () => void unlisten.then((stop) => stop());
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
            deleting={deleting}
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
              deleting={deleting}
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
      case "todos":
        return <TodosView deleting={deleting} {...shared} />;
      case "todoEditor":
        return (
          <TodoEditor
            key={`${selection.id}:${selection.projectId}`}
            todoId={selection.id}
            projectId={selection.projectId}
            onCancel={nav.back}
            {...shared}
          />
        );
      case "payments":
        return <PaymentsView tree={tree} actions={actions} />;
      case "assistant":
        return (
          <AssistantView
            assistant={assistant}
            tree={tree}
            session={session}
            run={run}
            onCopy={copy}
            actions={actions}
            composerRef={composerRef}
          />
        );
      case "paymentEditor":
        return (
          <PaymentEditor
            key={`${selection.id}:${selection.projectId}`}
            paymentId={selection.id}
            projectId={selection.projectId}
            tree={tree}
            actions={actions}
            onCancel={nav.back}
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
