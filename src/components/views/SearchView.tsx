import { useEffect, useState } from "react";
import {
  ArrowRight,
  Box,
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  Tag,
  type LucideIcon,
} from "lucide-react";
import type { Run, VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { api, type Item, type ItemSummary, type Tree } from "../../lib/api";
import { cx } from "../../lib/cx";
import { buildTagTree, hasTag, type TagNode } from "../../lib/tags";
import { toggled } from "../../lib/sets";
import { locationLabel, pathLabel } from "../../lib/tree";
import { itemIcon } from "../itemIcon";
import { Button } from "../ui/Button";
import { ItemBody } from "./ItemBody";
import { TagList } from "./TagList";
import styles from "./SearchView.module.css";
import viewStyles from "./View.module.css";

type Kind = "note" | "project" | "folder" | "tag";

const KINDS: { kind: Kind; label: string; icon: LucideIcon }[] = [
  { kind: "note", label: "Notes", icon: FileText },
  { kind: "project", label: "Projects", icon: Box },
  { kind: "folder", label: "Folders", icon: Folder },
  { kind: "tag", label: "Tags", icon: Tag },
];

type Props = {
  query: string;
  tree: Tree;
  session: Session;
  run: Run;
  actions: VaultActions;
  onCopy: (text: string) => Promise<void>;
};

/** Every tag path, parents included (`env` and `env/prod`). */
function allTags(nodes: TagNode[]): TagNode[] {
  return nodes.flatMap((node) => [node, ...allTags(node.children)]);
}

/** Drops tags whose parent also matched: "ai" lists `ai`, whose notes cover `ai/openai` too. */
function topmost(tags: TagNode[]): TagNode[] {
  const paths = new Set(tags.map((t) => t.path));
  return tags.filter((t) => {
    const segments = t.path.split("/");
    return !segments.slice(0, -1).some((_, i) => paths.has(segments.slice(0, i + 1).join("/")));
  });
}

/**
 * Search across notes, projects, folders and tags. Notes and tags expand in place so results
 * stay on screen; projects and folders navigate.
 */
export function SearchView({ query, tree, session, run, actions, onCopy }: Props) {
  const [kinds, setKinds] = useState<ReadonlySet<Kind>>(() => new Set(KINDS.map((k) => k.kind)));
  /** Expanded rows, keyed `note:<id>` or `tag:<path>`. */
  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());

  const q = query.trim().toLowerCase();
  const matches = (text: string) => text.toLowerCase().includes(q);

  const results = {
    project: tree.projects.filter((p) => matches(p.name)),
    folder: tree.folders.filter((f) => matches(f.name)),
    note: tree.items.filter((i) => matches(i.title) || i.tags.some(matches)),
    tag: topmost(allTags(buildTagTree(tree.items)).filter((t) => matches(t.path))),
  };

  const count = KINDS.filter((k) => kinds.has(k.kind)).reduce(
    (sum, k) => sum + results[k.kind].length,
    0,
  );

  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <header className={styles.header}>
          <div>
            <div className={viewStyles.eyebrow}>
              {count} {count === 1 ? "result" : "results"}
            </div>
            <h1 className={viewStyles.title}>Results for "{query.trim()}"</h1>
          </div>
          <div className={styles.filters}>
            {KINDS.map(({ kind, label, icon: Icon }) => (
              <button
                key={kind}
                type="button"
                aria-pressed={kinds.has(kind)}
                className={styles.filter}
                onClick={() => setKinds((k) => toggled(k, kind))}
              >
                <Icon size={14} strokeWidth={1.75} />
                {label}
                <span className={styles.filterCount}>{results[kind].length}</span>
              </button>
            ))}
          </div>
        </header>

        {count === 0 ? (
          <p className={styles.empty}>No results</p>
        ) : (
          <ul className={styles.list}>
            {kinds.has("project") &&
              results.project.map((project) => (
                <NavRow
                  key={`project:${project.id}`}
                  icon={Box}
                  title={project.name}
                  subtitle="Project"
                  onOpen={() => actions.selectProject(project.id)}
                />
              ))}
            {kinds.has("folder") &&
              results.folder.map((folder) => (
                <NavRow
                  key={`folder:${folder.id}`}
                  icon={Folder}
                  title={folder.name}
                  subtitle={pathLabel(tree, folder.projectId, folder.parentId)}
                  onOpen={() => actions.selectFolder(folder.id)}
                />
              ))}
            {kinds.has("note") &&
              results.note.map((item) => (
                <NoteRow
                  key={`note:${item.id}`}
                  item={item}
                  location={locationLabel(tree, item)}
                  open={open.has(`note:${item.id}`)}
                  onToggle={() => setOpen((o) => toggled(o, `note:${item.id}`))}
                  session={session}
                  run={run}
                  actions={actions}
                  onCopy={onCopy}
                />
              ))}
            {kinds.has("tag") &&
              results.tag.map((tag) => (
                <TagRow
                  key={`tag:${tag.path}`}
                  tag={tag}
                  notes={tree.items.filter((i) => hasTag(i, tag.path))}
                  tree={tree}
                  open={open.has(`tag:${tag.path}`)}
                  onToggle={() => setOpen((o) => toggled(o, `tag:${tag.path}`))}
                  actions={actions}
                />
              ))}
          </ul>
        )}
      </div>
    </div>
  );
}

type NavRowProps = { icon: LucideIcon; title: string; subtitle: string; onOpen: () => void };

function NavRow({ icon: Icon, title, subtitle, onOpen }: NavRowProps) {
  return (
    <li className={styles.entry}>
      <button type="button" className={styles.row} onClick={onOpen}>
        <Icon size={20} strokeWidth={1.75} className={styles.icon} />
        <span className={styles.text}>
          <span className={styles.title}>{title}</span>
          <span className={styles.subtitle}>{subtitle}</span>
        </span>
        <ChevronRight size={16} strokeWidth={1.75} className={styles.trail} />
      </button>
    </li>
  );
}

type NoteRowProps = {
  item: ItemSummary;
  location: string;
  open: boolean;
  onToggle: () => void;
  session: Session;
  run: Run;
  actions: VaultActions;
  onCopy: (text: string) => Promise<void>;
};

function NoteRow({ item, location, open, onToggle, session, run, actions, onCopy }: NoteRowProps) {
  const Icon = itemIcon(item);
  // Mounted on first open and kept, so collapsing animates instead of popping.
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    if (open) setLoaded(true);
  }, [open]);

  return (
    <li className={cx(styles.entry, open && styles.entryOpen)}>
      {/* A div, not a button: the tag chips inside are buttons themselves. */}
      <div
        role="button"
        tabIndex={0}
        className={styles.row}
        aria-expanded={open}
        onClick={onToggle}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggle();
          }
        }}
      >
        <Icon size={20} strokeWidth={1.75} className={styles.icon} />
        <span className={styles.text}>
          <span className={styles.title}>{item.title}</span>
          <span className={styles.subtitle}>{location}</span>
        </span>
        {item.tags.length > 0 && (
          <span className={styles.tags}>
            <TagList tags={item.tags} onSelect={actions.selectTag} />
          </span>
        )}
        <ChevronDown size={16} strokeWidth={1.75} className={cx(styles.trail, styles.chevron)} />
      </div>
      <div className={styles.panel}>
        <div className={styles.panelInner}>
          {loaded && (
            <NotePreview
              itemId={item.id}
              version={item.updatedAt}
              session={session}
              run={run}
              onCopy={onCopy}
              onGoTo={() => actions.openItem(item)}
            />
          )}
        </div>
      </div>
    </li>
  );
}

type PreviewProps = {
  itemId: number;
  version: number;
  session: Session;
  run: Run;
  onCopy: (text: string) => Promise<void>;
  onGoTo: () => void;
};

function NotePreview({ itemId, version, session, run, onCopy, onGoTo }: PreviewProps) {
  const [item, setItem] = useState<Item | null>(null);

  useEffect(() => {
    void run(async () => setItem(await api.getItem(itemId)));
  }, [itemId, version, run]);

  if (!item) return null;
  return (
    <div className={styles.preview}>
      <ItemBody key={item.updatedAt} item={item} session={session} run={run} onCopy={onCopy} />
      <div className={styles.previewFooter}>
        <Button variant="ghost" size="sm" icon={ArrowRight} onClick={onGoTo}>
          Go to note
        </Button>
      </div>
    </div>
  );
}

type TagRowProps = {
  tag: TagNode;
  notes: ItemSummary[];
  tree: Tree;
  open: boolean;
  onToggle: () => void;
  actions: VaultActions;
};

/** A tag that expands into the notes carrying it (or a tag nested below it). */
function TagRow({ tag, notes, tree, open, onToggle, actions }: TagRowProps) {
  return (
    <li className={cx(styles.entry, open && styles.entryOpen)}>
      <button type="button" className={styles.row} aria-expanded={open} onClick={onToggle}>
        <Tag size={20} strokeWidth={1.75} className={styles.icon} />
        <span className={styles.text}>
          <span className={styles.title}>{tag.path}</span>
          <span className={styles.subtitle}>
            {tag.count} {tag.count === 1 ? "note" : "notes"}
          </span>
        </span>
        <ChevronDown size={16} strokeWidth={1.75} className={cx(styles.trail, styles.chevron)} />
      </button>
      <div className={styles.panel}>
        <div className={styles.panelInner}>
          <div className={cx(styles.preview, styles.tagPreview)}>
            <ul className={styles.tagNotes}>
              {notes.map((note) => {
                const Icon = itemIcon(note);
                return (
                  <li key={note.id}>
                    <button
                      type="button"
                      className={styles.tagNote}
                      onClick={() => actions.openItem(note)}
                    >
                      <Icon size={16} strokeWidth={1.75} className={styles.icon} />
                      <span className={styles.title}>{note.title}</span>
                      <span className={styles.subtitle}>{locationLabel(tree, note)}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
            <div className={styles.previewFooter}>
              <Button
                variant="ghost"
                size="sm"
                icon={ArrowRight}
                onClick={() => actions.selectTag(tag.path)}
              >
                Open tag
              </Button>
            </div>
          </div>
        </div>
      </div>
    </li>
  );
}
