import { useEffect, useState } from "react";
import {
  Box,
  Copy,
  CopyPlus,
  Eye,
  EyeOff,
  KeyRound,
  LockKeyhole,
  Pencil,
  Trash2,
} from "lucide-react";
import type { Run, VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { api, type Item, type SaveItemInput, type Tree } from "../../lib/api";
import {
  parseBody,
  parseCopyable,
  parseTemplate,
  secretValues,
  toPlainText,
} from "../../lib/markup";
import { locationLabel } from "../../lib/tree";
import { Button } from "../ui/Button";
import { InlineBody } from "./InlineBody";
import { ItemEditor } from "./ItemEditor";
import { TagList } from "./TagList";
import styles from "./ItemView.module.css";
import viewStyles from "./View.module.css";

type Props = {
  itemId: number;
  tree: Tree;
  session: Session;
  run: Run;
  actions: VaultActions;
  onSave: (input: SaveItemInput) => Promise<void>;
  /** Writes to the clipboard and confirms with a toast. */
  onCopy: (text: string) => Promise<void>;
};

/** Secret index used for the whole body of a secure item. */
const WHOLE_BODY = 0;

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

export function ItemView({ itemId, tree, session, run, actions, onSave, onCopy }: Props) {
  const [item, setItem] = useState<Item | null>(null);
  const [editing, setEditing] = useState<SaveItemInput | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  // Decrypted body, fetched on the first unhide/copy. Lives only as long as this view.
  const [plaintext, setPlaintext] = useState<string | null>(null);
  const [shown, setShown] = useState<ReadonlySet<number>>(() => new Set());

  const summary = tree.items.find((i) => i.id === itemId);

  // Refetch whenever the tree reports a newer version; skip once the item is gone.
  const version = summary?.updatedAt;
  useEffect(() => {
    if (version === undefined) return;
    setPlaintext(null);
    setShown(new Set());
    void run(async () => setItem(await api.getItem(itemId)));
  }, [itemId, version, run]);

  // Drop decrypted content as soon as the unlock window closes.
  useEffect(() => {
    if (session.unlocked) return;
    setPlaintext(null);
    setShown(new Set());
  }, [session.unlocked]);

  if (!item || !summary) return null;

  const isProtected = item.secure || item.hasSecrets;

  /** The full body; asks for Touch ID only when the unlock window is closed. */
  const fullBody = async (): Promise<string> => {
    if (!isProtected) return item.body ?? "";
    if (plaintext !== null) return plaintext;
    const body = await session.withUnlock(() => api.revealItem(item.id));
    setPlaintext(body);
    return body;
  };

  const toggleSecret = (index: number) =>
    run(async () => {
      if (!shown.has(index)) await fullBody();
      setShown((prev) => {
        const next = new Set(prev);
        if (!next.delete(index)) next.add(index);
        return next;
      });
    });

  const copySecret = (index: number) =>
    run(async () => {
      const body = await fullBody();
      const value = item.secure ? body : secretValues(body)[index];
      if (value !== undefined) await onCopy(value);
    });

  const copyAll = () => run(async () => onCopy(toPlainText(await fullBody())));

  const startEdit = () =>
    run(async () => {
      const body = await fullBody();
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

  const segments = item.secure
    ? parseCopyable(plaintext ?? "")
    : item.hasSecrets && plaintext !== null
      ? parseBody(plaintext)
      : parseTemplate(item.body ?? "");
  const bodyHidden = item.secure && !shown.has(WHOLE_BODY);

  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <header className={viewStyles.header}>
          <div>
            <div className={viewStyles.eyebrow}>
              <Box size={14} strokeWidth={1.75} />
              {locationLabel(tree, summary)}
            </div>
            <h1 className={viewStyles.title}>
              {item.secure && (
                <KeyRound size={20} strokeWidth={1.75} className={viewStyles.titleIcon} />
              )}
              {item.title}
            </h1>
          </div>
          <div className={viewStyles.actions}>
            <Button variant="ghost" size="sm" icon={Pencil} onClick={startEdit}>
              Edit
            </Button>
            <Button
              variant="ghost"
              size="sm"
              icon={CopyPlus}
              onClick={() => actions.duplicateItem(item.id)}
            >
              Duplicate
            </Button>
            <Button
              variant={confirmingDelete ? "danger" : "ghost"}
              size="sm"
              icon={Trash2}
              onClick={() =>
                confirmingDelete ? actions.deleteItem(item.id) : setConfirmingDelete(true)
              }
              onMouseLeave={() => setConfirmingDelete(false)}
            >
              {confirmingDelete ? "Confirm delete" : "Delete"}
            </Button>
          </div>
        </header>

        {item.tags.length > 0 && <TagList tags={item.tags} onSelect={actions.selectTag} />}

        <div className={styles.body}>
          <div className={styles.bodyToolbar}>
            {item.secure && (
              <Button
                variant="ghost"
                size="sm"
                icon={bodyHidden ? Eye : EyeOff}
                onClick={() => toggleSecret(WHOLE_BODY)}
              >
                {bodyHidden ? "Unhide" : "Hide"}
              </Button>
            )}
            <Button variant="ghost" size="sm" icon={Copy} onClick={copyAll}>
              Copy all
            </Button>
          </div>
          {bodyHidden ? (
            <div className={styles.hidden}>
              <span className={styles.mask}>
                <LockKeyhole size={12} strokeWidth={2} />
                hidden
              </span>
            </div>
          ) : (
            <InlineBody
              segments={segments}
              shown={shown}
              onToggleSecret={toggleSecret}
              onCopySecret={copySecret}
              onCopyText={(text) => void run(() => onCopy(text))}
            />
          )}
        </div>

        <p className={viewStyles.meta}>Updated {dateFormat.format(item.updatedAt)}</p>
      </div>
    </div>
  );
}
