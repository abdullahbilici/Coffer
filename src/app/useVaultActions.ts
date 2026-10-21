import { useCallback, useMemo, useState } from "react";
import type { TreeEdit } from "../components/sidebar/ProjectTree";
import type { useExpansion } from "../hooks/useExpansion";
import type { useNavigation } from "../hooks/useNavigation";
import type { Session } from "../hooks/useSession";
import { api, type ItemSummary, type SaveItemInput, type Tree } from "../lib/api";
import { motion } from "../lib/motion";
import { nodeKey } from "../lib/selection";
import type { Run, VaultActions } from "./actions";

type Deps = {
  tree: Tree | null;
  reload: () => Promise<void>;
  session: Session;
  run: Run;
  showError: (error: unknown) => void;
  nav: ReturnType<typeof useNavigation>;
  expansion: ReturnType<typeof useExpansion>;
  setTreeEdit: (edit: TreeEdit | null) => void;
};

/** Every vault mutation and navigation the UI can trigger, plus the state they drive. */
export function useVaultActions({
  tree,
  reload,
  session,
  run,
  showError,
  nav,
  expansion,
  setTreeEdit,
}: Deps) {
  /** Tree node keys closing before they are deleted (see `nodeKey`). */
  const [deleting, setDeleting] = useState<ReadonlySet<string>>(() => new Set());
  const { select, setSelection } = nav;
  const { expandTo } = expansion;

  /** Runs a change behind the unlock window, reloads the tree, then `then` with the result. */
  const mutate = useCallback(
    <T>(change: () => Promise<T>, then?: (result: T) => void) =>
      run(async () => {
        const result = await session.withUnlock(change);
        await reload();
        then?.(result);
      }),
    [run, session, reload],
  );

  /** Asks for Touch ID if needed before `then`, e.g. before opening an editor. */
  const guarded = useCallback(
    (then: () => void) =>
      run(async () => {
        await session.ensureUnlocked();
        then();
      }),
    [run, session],
  );

  /**
   * Deletes after the tree row (and its open children) has closed. Touch ID comes first, so a
   * cancelled prompt never starts the animation; a failed delete brings the row back.
   */
  const deleteWithExit = useCallback(
    async (key: string, remove: () => Promise<void>) => {
      await session.ensureUnlocked();
      setDeleting((d) => new Set(d).add(key));
      try {
        await new Promise((resolve) => setTimeout(resolve, motion().baseMs));
        await session.withUnlock(remove);
        await reload();
      } finally {
        setDeleting((d) => {
          const next = new Set(d);
          next.delete(key);
          return next;
        });
      }
    },
    [session, reload],
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
      guard: guarded,
      newItem: (projectId, folderId) =>
        guarded(() => select({ kind: "newItem", projectId, folderId })),

      createProject: (name) =>
        mutate(
          () => api.createProject(name),
          (project) => select({ kind: "project", id: project.id }),
        ),
      renameProject: (id, name) => mutate(() => api.renameProject(id, name)),
      deleteProject: (id) =>
        run(async () => {
          await deleteWithExit(nodeKey.project(id), () => api.deleteProject(id));
          setSelection(null);
        }),

      startCreateFolder: (projectId, parentId) =>
        guarded(() => {
          expandTo(projectId, parentId);
          setTreeEdit({ type: "createFolder", projectId, parentId });
        }),
      createFolder: (projectId, parentId, name) =>
        mutate(() => api.createFolder(projectId, parentId, name)),
      renameFolder: (id, name) => mutate(() => api.renameFolder(id, name)),
      // After a move, open the destination so the moved row is visible where it landed.
      moveFolder: (id, projectId, parentId) =>
        mutate(
          () => api.moveFolder(id, projectId, parentId),
          () => expandTo(projectId, parentId),
        ),
      moveItem: (id, projectId, folderId) =>
        mutate(
          () => api.moveItem(id, projectId, folderId),
          () => expandTo(projectId, folderId),
        ),
      deleteFolder: (id) =>
        run(async () => {
          await deleteWithExit(nodeKey.folder(id), () => api.deleteFolder(id));
          setSelection(null);
        }),

      setPinned: (pin, pinned) => mutate(() => api.setPinned(pin, pinned)),
      reorderPins: (order) => mutate(() => api.reorderPins(order)),
      duplicateItem: (id) => mutate(() => api.duplicateItem(id), openItem),
      deleteItem: (id) =>
        run(async () => {
          await deleteWithExit(nodeKey.item(id), () => api.deleteItem(id));
          setSelection((s) => (s?.kind === "item" && s.id === id ? null : s));
        }),
    }),
    [
      tree,
      run,
      mutate,
      guarded,
      deleteWithExit,
      select,
      setSelection,
      openItem,
      expandTo,
      setTreeEdit,
    ],
  );

  return { actions, saveItem, deleting };
}
