import { useState, type MouseEvent } from "react";
import { Ellipsis, Pencil, Trash2 } from "lucide-react";
import type { Conversation } from "../../lib/api";
import { cx } from "../../lib/cx";
import { motion } from "../../lib/motion";
import { Collapse } from "../ui/Collapse";
import { IconButton } from "../ui/IconButton";
import { CONFIRM_DELETE, Menu, type MenuAnchor } from "../ui/Menu";
import styles from "./Assistant.module.css";

type Props = {
  conversations: Conversation[];
  selectedId: number | null;
  onOpen: (id: number) => void;
  onRename: (id: number, title: string) => void;
  /** Resolves once the delete finished or failed. */
  onDelete: (id: number) => Promise<void>;
};

/** Conversations, most recently active first. Rename and delete from the row's menu. */
export function ConversationList({ conversations, selectedId, onOpen, ...props }: Props) {
  const [menu, setMenu] = useState<{ id: number; anchor: MenuAnchor } | null>(null);
  const [renaming, setRenaming] = useState<number | null>(null);
  const [leaving, setLeaving] = useState<number | null>(null);

  // Right-click opens at the cursor; the "more" button opens below itself.
  const openMenu = (event: MouseEvent<HTMLElement>, id: number) => {
    event.preventDefault();
    event.stopPropagation();
    const rect = event.currentTarget.getBoundingClientRect();
    const anchor =
      event.type === "contextmenu"
        ? { x: event.clientX, y: event.clientY }
        : { x: rect.left, y: rect.bottom + 4 };
    setMenu({ id, anchor });
  };

  // The row closes first, then the conversation goes; a failed delete brings it back.
  const remove = (id: number) => {
    setLeaving(id);
    setTimeout(() => void props.onDelete(id).finally(() => setLeaving(null)), motion().baseMs);
  };

  if (conversations.length === 0) return <p className={styles.listEmpty}>No chats</p>;

  return (
    <nav className={styles.list} aria-label="Chats">
      {conversations.map((c) => (
        <Collapse key={c.id} leaving={leaving === c.id}>
          {renaming === c.id ? (
            <RenameRow
              title={c.title}
              onSubmit={(title) => {
                setRenaming(null);
                props.onRename(c.id, title);
              }}
              onCancel={() => setRenaming(null)}
            />
          ) : (
            <div
              role="button"
              tabIndex={0}
              className={cx(styles.row, c.id === selectedId && styles.rowSelected)}
              aria-current={c.id === selectedId ? "page" : undefined}
              onClick={() => onOpen(c.id)}
              onKeyDown={(e) => e.key === "Enter" && onOpen(c.id)}
              onContextMenu={(e) => openMenu(e, c.id)}
            >
              <span className={styles.rowLabel}>{c.title}</span>
              {c.replying && <span className={styles.replyingDot} aria-label="Answering" />}
              <span className={styles.rowActions}>
                <IconButton
                  icon={Ellipsis}
                  label="More"
                  size="sm"
                  onClick={(e) => openMenu(e, c.id)}
                />
              </span>
            </div>
          )}
        </Collapse>
      ))}
      {menu && (
        <Menu
          anchor={menu.anchor}
          items={[
            { label: "Rename", icon: Pencil, onSelect: () => setRenaming(menu.id) },
            {
              label: "Delete",
              icon: Trash2,
              danger: true,
              confirmLabel: CONFIRM_DELETE,
              onSelect: () => remove(menu.id),
            },
          ]}
          onClose={() => setMenu(null)}
        />
      )}
    </nav>
  );
}

type RenameRowProps = { title: string; onSubmit: (title: string) => void; onCancel: () => void };

function RenameRow({ title, onSubmit, onCancel }: RenameRowProps) {
  const [value, setValue] = useState(title);
  const finish = () => {
    const next = value.trim();
    if (next && next !== title) onSubmit(next);
    else onCancel();
  };
  return (
    <div className={cx(styles.row, styles.rowEditing)}>
      <input
        className={styles.rowInput}
        value={value}
        autoFocus
        spellCheck={false}
        onFocus={(e) => e.target.select()}
        onChange={(e) => setValue(e.target.value)}
        onBlur={finish}
        onKeyDown={(e) => {
          if (e.key === "Enter") finish();
          if (e.key === "Escape") {
            // Handled here, so Esc doesn't also clear the main pane.
            e.preventDefault();
            onCancel();
          }
        }}
      />
    </div>
  );
}
