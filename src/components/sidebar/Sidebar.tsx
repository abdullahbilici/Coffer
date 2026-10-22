import { useEffect, useMemo, useRef } from "react";
import { ChevronsDownUp, ListTodo, Plus, Search, Wallet, X, type LucideIcon } from "lucide-react";
import type { VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import type { Tree } from "../../lib/api";
import type { Selection, TreeSection } from "../../lib/selection";
import { cx } from "../../lib/cx";
import { today } from "../../lib/dates";
import { pinnedSet } from "../../lib/pins";
import { buildTagTree } from "../../lib/tags";
import { buildProjectTree } from "../../lib/tree";
import { IconButton } from "../ui/IconButton";
import { Kbd } from "../ui/Kbd";
import { ProjectTree, type TreeEdit } from "./ProjectTree";
import { SessionBar } from "./SessionBar";
import { TagTree } from "./TagTree";
import { PinnedList } from "./PinnedList";
import { SplitPanes } from "./SplitPanes";
import { TreeDndProvider } from "./TreeDnd";
import styles from "./Sidebar.module.css";

type Props = {
  tree: Tree;
  selection: Selection;
  expanded: ReadonlySet<string>;
  onToggle: (key: string) => void;
  onExpand: (keys: string[]) => void;
  onCollapseAll: (section: TreeSection) => void;
  /** Tree node keys animating out before deletion. */
  deleting: ReadonlySet<string>;
  query: string;
  onQueryChange: (query: string) => void;
  edit: TreeEdit | null;
  onEdit: (edit: TreeEdit | null) => void;
  session: Session;
  onLock: () => void;
  onSettings: () => void;
  actions: VaultActions;
};

export function Sidebar(props: Props) {
  const { tree, selection, expanded, onToggle, onExpand, query, onQueryChange, actions } = props;
  const { edit, onEdit } = props;
  const searchRef = useRef<HTMLInputElement>(null);

  const projectNodes = useMemo(() => buildProjectTree(tree), [tree]);
  const tagNodes = useMemo(() => buildTagTree(tree.items), [tree.items]);
  const isPinned = useMemo(() => pinnedSet(tree), [tree]);
  const now = today();
  const dueTodos = tree.todos.filter(
    (t) => !t.done && t.dueDate !== null && t.dueDate <= now,
  ).length;

  // Cmd+K or Cmd+F focuses search.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();
      if (event.metaKey && (key === "k" || key === "f")) {
        event.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <aside className={styles.sidebar}>
      <div className={styles.titlebar} data-tauri-drag-region />

      <div className={styles.search}>
        <Search size={14} strokeWidth={1.75} className={styles.searchIcon} />
        <input
          ref={searchRef}
          className={styles.searchInput}
          placeholder="Search"
          value={query}
          spellCheck={false}
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && onQueryChange("")}
        />
        {query ? (
          <IconButton icon={X} label="Clear search" size="sm" onClick={() => onQueryChange("")} />
        ) : (
          <Kbd letter="K" />
        )}
      </div>

      <nav className={styles.places} aria-label="Agenda">
        <PlaceRow
          icon={ListTodo}
          label="Todos"
          selected={selection?.kind === "todos"}
          count={dueTodos}
          onClick={actions.selectTodos}
        />
        <PlaceRow
          icon={Wallet}
          label="Payments"
          selected={selection?.kind === "payments"}
          onClick={actions.selectPayments}
        />
      </nav>

      <SplitPanes
        panes={[
          {
            id: "pinned",
            title: "Pinned",
            children: <PinnedList tree={tree} selection={selection} actions={actions} />,
          },
          {
            id: "projects",
            title: "Projects",
            fill: true,
            actions: (
              <>
                <CollapseAll section="projects" {...props} />
                <IconButton
                  icon={Plus}
                  label="New project"
                  size="sm"
                  onClick={() => actions.guard(() => onEdit({ type: "createProject" }))}
                />
              </>
            ),
            children: (
              <TreeDndProvider
                tree={tree}
                expanded={expanded}
                onExpand={onExpand}
                authorize={props.session.ensureUnlocked}
                onMove={(subject, { projectId, folderId }) =>
                  subject.kind === "item"
                    ? actions.moveItem(subject.id, projectId, folderId)
                    : actions.moveFolder(subject.id, projectId, folderId)
                }
              >
                <ProjectTree
                  nodes={projectNodes}
                  expanded={expanded}
                  selection={selection}
                  edit={edit}
                  onEdit={onEdit}
                  onToggle={onToggle}
                  onExpand={onExpand}
                  isPinned={isPinned}
                  deleting={props.deleting}
                  actions={actions}
                />
              </TreeDndProvider>
            ),
          },
          {
            id: "tags",
            title: "Tags",
            actions: <CollapseAll section="tags" {...props} />,
            children:
              tagNodes.length > 0 ? (
                <TagTree
                  nodes={tagNodes}
                  expanded={expanded}
                  selection={selection}
                  onToggle={onToggle}
                  onExpand={onExpand}
                  onSelect={actions.selectTag}
                />
              ) : (
                <p className={styles.empty}>No tags</p>
              ),
          },
        ]}
      />

      <SessionBar session={props.session} onLock={props.onLock} onSettings={props.onSettings} />
    </aside>
  );
}

function CollapseAll({ section, onCollapseAll }: Props & { section: TreeSection }) {
  return (
    <IconButton
      icon={ChevronsDownUp}
      label="Collapse all"
      size="sm"
      onClick={() => onCollapseAll(section)}
    />
  );
}

type PlaceRowProps = {
  icon: LucideIcon;
  label: string;
  selected: boolean;
  /** Open todos due today or earlier. */
  count?: number;
  onClick: () => void;
};

function PlaceRow({ icon: Icon, label, selected, count, onClick }: PlaceRowProps) {
  return (
    <button
      type="button"
      className={cx(styles.place, selected && styles.placeSelected)}
      aria-current={selected ? "page" : undefined}
      onClick={onClick}
    >
      <Icon size={16} strokeWidth={1.75} className={styles.placeIcon} />
      <span className={styles.placeLabel}>{label}</span>
      {count !== undefined && count > 0 && <span className={styles.placeCount}>{count}</span>}
    </button>
  );
}
