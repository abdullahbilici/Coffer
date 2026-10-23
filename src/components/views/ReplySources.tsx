import { Box, type LucideIcon } from "lucide-react";
import type { VaultActions } from "../../app/actions";
import type { Tree } from "../../lib/api";
import type { ReplySource } from "../../lib/assistant";
import { cx } from "../../lib/cx";
import { nodeKey } from "../../lib/selection";
import { itemIcon } from "../itemIcon";
import { Breadcrumb, pathCrumbs, type Crumb } from "./Breadcrumb";
import styles from "./Assistant.module.css";

type Props = {
  sources: ReplySource[];
  tree: Tree;
  actions: VaultActions;
  /** The source of the secret chip under the pointer. */
  active: ReplySource | null;
};

type Row = { source: ReplySource; icon: LucideIcon; crumbs: Crumb[] };

/** Where a reply's answer came from: a path per note or project, each part navigating there. */
export function ReplySources({ sources, tree, actions, active }: Props) {
  const rows = sources.flatMap((source): Row[] => {
    if (source.kind === "project") {
      const crumbs = pathCrumbs(tree, source.id, null, actions);
      return crumbs[0].label ? [{ source, icon: Box, crumbs }] : [];
    }
    const item = tree.items.find((i) => i.id === source.id);
    if (!item) return [];
    const crumbs: Crumb[] = [
      ...pathCrumbs(tree, item.projectId, item.folderId, actions),
      { key: nodeKey.item(item.id), label: item.title, onClick: () => actions.openItem(item) },
    ];
    return [{ source, icon: itemIcon(item), crumbs }];
  });
  if (rows.length === 0) return null;
  return (
    <div className={styles.sources}>
      {rows.map(({ source, icon: Icon, crumbs }) => (
        <div
          key={`${source.kind}:${source.id}`}
          className={cx(
            styles.source,
            active?.kind === source.kind && active.id === source.id && styles.sourceActive,
          )}
        >
          <Icon size={14} strokeWidth={1.75} className={styles.sourceIcon} />
          <Breadcrumb crumbs={crumbs} variant="inline" />
        </div>
      ))}
    </div>
  );
}
