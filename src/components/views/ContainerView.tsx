import { FilePlus, Folder, FolderPlus, LayoutGrid, List, Pencil } from "lucide-react";
import type { Run, VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { useStoredState } from "../../hooks/useStoredState";
import type { Tree } from "../../lib/api";
import { folderPath } from "../../lib/tree";
import { itemIcon } from "../itemIcon";
import { Button } from "../ui/Button";
import { Segmented } from "../ui/Segmented";
import { Breadcrumb, type Crumb } from "./Breadcrumb";
import { EntryList, type Entry } from "./EntryList";
import { FolderGrid } from "./FolderGrid";
import { ProjectOverview, useProjectOverview, type Overview } from "./ProjectOverview";
import styles from "./ContainerView.module.css";
import viewStyles from "./View.module.css";

type ContentsView = "grid" | "list";

const VIEW_OPTIONS = [
  { value: "grid" as const, label: "Folder tiles", icon: LayoutGrid },
  { value: "list" as const, label: "List", icon: List },
];

type Props = {
  projectId: number;
  /** `null` shows the project root. */
  folderId: number | null;
  tree: Tree;
  run: Run;
  actions: VaultActions;
  session: Session;
  onCopy: (text: string) => Promise<void>;
};

/**
 * A project or folder: breadcrumb, then for a project root its markdown overview, then its
 * subfolders as tiles, then its notes as a list.
 */
export function ContainerView(props: Props) {
  return props.folderId === null ? (
    <ProjectRoot {...props} />
  ) : (
    <Container {...props} overview={null} />
  );
}

function ProjectRoot(props: Props) {
  const overview = useProjectOverview(props);
  return <Container {...props} overview={overview} />;
}

function Container({
  projectId,
  folderId,
  tree,
  actions,
  overview,
}: Props & { overview: Overview | null }) {
  const [view, setView] = useStoredState<ContentsView>("coffer.contents.view", "grid");
  const project = tree.projects.find((p) => p.id === projectId);
  const path = folderPath(tree.folders, folderId);
  const folder = path.at(-1) ?? null;
  if (!project || (folderId !== null && !folder)) return null;

  const items = tree.items.filter((i) => i.projectId === projectId && i.folderId === folderId);
  const subfolders = tree.folders.filter(
    (f) => f.projectId === projectId && f.parentId === folderId,
  );

  const countIn = (id: number) => {
    const items = tree.items.filter((i) => i.folderId === id).length;
    const folders = tree.folders.filter((f) => f.parentId === id).length;
    return [plural(items, "item"), folders > 0 && plural(folders, "folder")]
      .filter(Boolean)
      .join(", ");
  };

  // The full path is the page header; the last crumb is where we are.
  const crumbs: Crumb[] = [
    {
      key: `project:${project.id}`,
      label: project.name,
      onClick: folder ? () => actions.selectProject(project.id) : undefined,
    },
    ...path.map((f) => ({
      key: `folder:${f.id}`,
      label: f.name,
      onClick: f.id === folder?.id ? undefined : () => actions.selectFolder(f.id),
    })),
  ];

  // List view: folders as rows with a boxed icon, ahead of the notes.
  const folderEntries: Entry[] = subfolders.map((sub) => ({
    key: `folder:${sub.id}`,
    icon: Folder,
    title: sub.name,
    subtitle: countIn(sub.id),
    boxed: true,
    onOpen: () => actions.selectFolder(sub.id),
  }));

  const entries: Entry[] = items.map((item) => ({
    key: `item:${item.id}`,
    icon: itemIcon(item),
    title: item.title,
    tags: item.tags,
    onOpen: () => actions.openItem(item),
  }));

  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <header className={styles.header}>
          <h1 className={styles.path}>
            <Breadcrumb variant="path" crumbs={crumbs} />
          </h1>
          <div className={viewStyles.actions}>
            <Segmented value={view} options={VIEW_OPTIONS} onChange={setView} />
            {overview && overview.readme !== null && overview.draft === null && (
              <Button
                variant="ghost"
                size="sm"
                icon={Pencil}
                onClick={() => overview.startEdit(`# ${project.name}\n\n`)}
              >
                {overview.readme ? "Edit overview" : "Write overview"}
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              icon={FilePlus}
              onClick={() => actions.newItem(projectId, folderId)}
            >
              New item
            </Button>
            <Button
              variant="ghost"
              size="sm"
              icon={FolderPlus}
              onClick={() => actions.startCreateFolder(projectId, folderId)}
            >
              New folder
            </Button>
          </div>
        </header>

        {overview && <ProjectOverview overview={overview} />}

        {/* One tray for everything inside this project or folder, so it reads as its contents. */}
        <section className={styles.contents} aria-label="Contents">
          {view === "grid" ? (
            <>
              {subfolders.length > 0 && (
                <FolderGrid
                  folders={subfolders}
                  describe={(f) => countIn(f.id)}
                  onOpen={(f) => actions.selectFolder(f.id)}
                />
              )}
              {entries.length > 0 && (
                <EntryList entries={entries} emptyText="" onSelectTag={actions.selectTag} />
              )}
            </>
          ) : (
            (entries.length > 0 || subfolders.length > 0) && (
              <EntryList
                entries={[...folderEntries, ...entries]}
                emptyText=""
                onSelectTag={actions.selectTag}
              />
            )
          )}
          {entries.length === 0 && subfolders.length === 0 && <p className={styles.empty}>Empty</p>}
        </section>
      </div>
    </div>
  );
}

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}
