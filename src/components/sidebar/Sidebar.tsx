import { useEffect, useMemo, useRef, type ReactNode } from "react";
import { ChevronsDownUp, Plus, Search, X } from "lucide-react";
import type { VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import type { Tree } from "../../lib/api";
import { inSection, type Selection, type TreeSection } from "../../lib/selection";
import { buildTagTree } from "../../lib/tags";
import { buildProjectTree } from "../../lib/tree";
import { IconButton } from "../ui/IconButton";
import { Kbd } from "../ui/Kbd";
import { ProjectTree, type TreeEdit } from "./ProjectTree";
import { SessionBar } from "./SessionBar";
import { TagTree } from "./TagTree";
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

      <div className={styles.scroll}>
        <Section
          title="Projects"
          action={
            <>
              <CollapseAll section="projects" {...props} />
              <IconButton
                icon={Plus}
                label="New project"
                size="sm"
                onClick={() => onEdit({ type: "createProject" })}
              />
            </>
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
            actions={actions}
          />
        </Section>

        <Section title="Tags" action={<CollapseAll section="tags" {...props} />}>
          {tagNodes.length > 0 ? (
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
          )}
        </Section>
      </div>

      <SessionBar session={props.session} onLock={props.onLock} />
    </aside>
  );
}

function CollapseAll({ section, expanded, onCollapseAll }: Props & { section: TreeSection }) {
  const anyOpen = [...expanded].some((key) => inSection(key, section));
  return (
    <IconButton
      icon={ChevronsDownUp}
      label="Collapse all"
      size="sm"
      disabled={!anyOpen}
      onClick={() => onCollapseAll(section)}
    />
  );
}

function Section({
  title,
  action,
  children,
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className={styles.section}>
      <header className={styles.sectionHeader}>
        <span>{title}</span>
        <span className={styles.sectionActions}>{action}</span>
      </header>
      {children}
    </section>
  );
}
