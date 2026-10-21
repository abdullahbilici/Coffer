import { Box, FilePlus, Folder, FolderOpen, FolderPlus, Pencil } from "lucide-react";
import type { Run, VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import type { Tree } from "../../lib/api";
import { folderPath } from "../../lib/tree";
import { itemIcon } from "../itemIcon";
import { Button } from "../ui/Button";
import { Breadcrumb } from "./Breadcrumb";
import { EntryList, type Entry } from "./EntryList";
import { ProjectOverview, useProjectOverview, type Overview } from "./ProjectOverview";
import styles from "./ContainerView.module.css";
import viewStyles from "./View.module.css";

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
 * subfolders and items (folders first).
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

  const entries: Entry[] = [
    ...subfolders.map((sub) => ({
      key: `folder:${sub.id}`,
      icon: Folder,
      title: sub.name,
      subtitle: countIn(sub.id),
      onOpen: () => actions.selectFolder(sub.id),
    })),
    ...items.map((item) => ({
      key: `item:${item.id}`,
      icon: itemIcon(item),
      title: item.title,
      tags: item.tags,
      onOpen: () => actions.openItem(item),
    })),
  ];

  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <header className={viewStyles.header}>
          <div>
            {folder ? (
              <Breadcrumb project={project} folders={path.slice(0, -1)} actions={actions} />
            ) : (
              <span className={styles.rootLabel}>Project</span>
            )}
            <h1 className={viewStyles.title}>
              {folder ? (
                <FolderOpen size={22} strokeWidth={1.75} className={viewStyles.titleIcon} />
              ) : (
                <Box size={22} strokeWidth={1.75} className={viewStyles.titleIcon} />
              )}
              {folder?.name ?? project.name}
            </h1>
          </div>
          <div className={viewStyles.actions}>
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

        <EntryList entries={entries} emptyText="Empty" onSelectTag={actions.selectTag} />
      </div>
    </div>
  );
}

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}
