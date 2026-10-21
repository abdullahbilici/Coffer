import { useEffect, useMemo, useRef } from "react";
import { ChevronsDownUp, Plus, Search, X } from "lucide-react";
import type { VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import type { Tree } from "../../lib/api";
import type { Selection, TreeSection } from "../../lib/selection";
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
  query: string;
  onQueryChange: (query: string) => void;
  edit: TreeEdit | null;
  onEdit: (edit: TreeEdit | null) => void;
  session: Session;
  onLock: () => void;
  actions: VaultActions;
};

export function Sidebar(props: Props) {
  const { tree, selection, expanded, onToggle, onExpand, query, onQueryChange, actions } = props;
  const { edit, onEdit } = props;
  const searchRef = useRef<HTMLInputElement>(null);

  const projectNodes = useMemo(() => buildProjectTree(tree), [tree]);
  const tagNodes = useMemo(() => buildTagTree(tree.items), [tree.items]);
  const isPinned = useMemo(() => pinnedSet(tree), [tree]);

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

      <SplitPanes
        panes={[
          {
            id: "pinned",
            title: "Pinned",
            defaultWeight: 0.22,
            children: <PinnedList tree={tree} selection={selection} actions={actions} />,
          },
          {
            id: "projects",
            title: "Projects",
            defaultWeight: 0.53,
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
                  actions={actions}
                />
              </TreeDndProvider>
            ),
          },
          {
            id: "tags",
            title: "Tags",
            defaultWeight: 0.25,
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

      <SessionBar session={props.session} onLock={props.onLock} />
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
