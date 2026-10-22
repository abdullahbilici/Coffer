import { useEffect, useState } from "react";
import { CopyPlus, KeyRound, Pencil, Pin, PinOff } from "lucide-react";
import type { Copy, Run, VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { api, type Item, type SaveItemInput, type Tree } from "../../lib/api";
import { pinnedSet } from "../../lib/pins";
import { Button } from "../ui/Button";
import { DeleteButton } from "../ui/DeleteButton";
import { Breadcrumb, pathCrumbs } from "./Breadcrumb";
import { ItemBody } from "./ItemBody";
import { ItemEditor } from "./ItemEditor";
import { TagList } from "./TagList";
import viewStyles from "./View.module.css";

type Props = {
  itemId: number;
  tree: Tree;
  session: Session;
  run: Run;
  actions: VaultActions;
  onSave: (input: SaveItemInput) => Promise<void>;
  /** Writes to the clipboard and confirms with a toast. */
  onCopy: Copy;
};

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

export function ItemView({ itemId, tree, session, run, actions, onSave, onCopy }: Props) {
  const [item, setItem] = useState<Item | null>(null);
  const [editing, setEditing] = useState<SaveItemInput | null>(null);
  const summary = tree.items.find((i) => i.id === itemId);

  // Refetch whenever the tree reports a newer version; skip once the item is gone.
  const version = summary?.updatedAt;
  useEffect(() => {
    if (version === undefined) return;
    void run(async () => setItem(await api.getItem(itemId)));
  }, [itemId, version, run]);

  if (!item || !summary) return null;
  const project = tree.projects.find((p) => p.id === summary.projectId);
  const pinned = pinnedSet(tree)("item", item.id);

  const startEdit = () =>
    run(async () => {
      await session.ensureUnlocked();
      const body =
        item.secure || item.hasSecrets
          ? await session.withUnlock(() => api.revealItem(item.id))
          : (item.body ?? "");
      setEditing({
        id: item.id,
        projectId: item.projectId,
        folderId: item.folderId,
        title: item.title,
        body,
        secure: item.secure,
        tags: item.tags,
      });
    });

  if (editing) {
    return (
      <ItemEditor
        key={item.id}
        tree={tree}
        initial={editing}
        heading="Edit item"
        onCancel={() => setEditing(null)}
        onSave={async (input) => {
          await onSave(input);
          setEditing(null);
        }}
      />
    );
  }

  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <header className={viewStyles.header}>
          <div>
            {project && (
              <Breadcrumb crumbs={pathCrumbs(tree, summary.projectId, summary.folderId, actions)} />
            )}
          </div>
          <div className={viewStyles.actions}>
            <Button variant="ghost" size="sm" icon={Pencil} onClick={startEdit}>
              Edit
            </Button>
            <Button
              variant="ghost"
              size="sm"
              icon={pinned ? PinOff : Pin}
              onClick={() => actions.setPinned({ kind: "item", id: item.id }, !pinned)}
            >
              {pinned ? "Unpin" : "Pin"}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              icon={CopyPlus}
              onClick={() => actions.duplicateItem(item.id)}
            >
              Duplicate
            </Button>
            <DeleteButton onDelete={() => actions.deleteItem(item.id)} />
          </div>
        </header>

        {item.tags.length > 0 && <TagList tags={item.tags} onSelect={actions.selectTag} />}

        <ItemBody
          key={item.updatedAt}
          item={item}
          session={session}
          run={run}
          onCopy={onCopy}
          title={
            <>
              {item.secure && (
                <KeyRound size={14} strokeWidth={1.75} className={viewStyles.titleIcon} />
              )}
              {item.title}
            </>
          }
        />

        <p className={viewStyles.meta}>Updated {dateFormat.format(item.updatedAt)}</p>
      </div>
    </div>
  );
}
