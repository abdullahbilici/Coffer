import { useCallback, useState } from "react";
import type { Tree } from "../lib/api";
import { inSection, nodeKey, type TreeSection } from "../lib/selection";
import { toggled } from "../lib/sets";
import { folderPath } from "../lib/tree";

/** Which sidebar nodes are open. Everything starts collapsed. */
export function useExpansion(tree: Tree | null) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());

  const toggle = useCallback((key: string) => setExpanded((prev) => toggled(prev, key)), []);

  const expand = useCallback((keys: string[]) => {
    setExpanded((prev) => (keys.every((k) => prev.has(k)) ? prev : new Set([...prev, ...keys])));
  }, []);

  const collapseAll = useCallback((section: TreeSection) => {
    setExpanded((prev) => new Set([...prev].filter((key) => !inSection(key, section))));
  }, []);

  /** Opens a project and the folders down to `folderId` so that location is visible. */
  const expandTo = useCallback(
    (projectId: number, folderId: number | null) => {
      const folders = tree ? folderPath(tree.folders, folderId) : [];
      expand([nodeKey.project(projectId), ...folders.map((f) => nodeKey.folder(f.id))]);
    },
    [tree, expand],
  );

  return { expanded, toggle, expand, collapseAll, expandTo };
}
