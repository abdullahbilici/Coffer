import { useCallback, useEffect, useRef, useState } from "react";
import type { Tree } from "../lib/api";
import type { Selection } from "../lib/selection";
import { hasTag } from "../lib/tags";
import { useHistory, type Place } from "./useHistory";

const EDITABLE = "input, textarea, select, [contenteditable]";

/** Whether a remembered place still exists (deleted targets are skipped by back/forward). */
function placeExists(tree: Tree | null, { selection }: Place): boolean {
  if (!tree || !selection) return true;
  switch (selection.kind) {
    case "project":
      return tree.projects.some((p) => p.id === selection.id);
    case "folder":
      return tree.folders.some((f) => f.id === selection.id);
    case "item":
      return tree.items.some((i) => i.id === selection.id);
    case "tag":
      return tree.items.some((i) => hasTag(i, selection.path));
    case "todos":
    case "payments":
      return true;
    case "newItem":
    case "todoEditor":
    case "paymentEditor":
      return false;
  }
}

/**
 * What the main pane shows, the search query, and back/forward history with its shortcuts:
 * Cmd+[ / Cmd+] by physical key (Cmd+Ğ / Cmd+Ü on a Turkish keyboard), Cmd+arrows outside
 * text fields, mouse buttons 3/4, and Esc to clear the pane.
 */
export function useNavigation(tree: Tree | null) {
  const [selection, setSelection] = useState<Selection>(null);
  const [query, setQuery] = useState("");

  const treeRef = useRef(tree);
  treeRef.current = tree;
  const history = useHistory(
    (place) => {
      setSelection(place.selection);
      setQuery(place.query);
    },
    (place) => placeExists(treeRef.current, place),
  );
  const { record } = history;

  /** Navigates and records the move, so Back returns here (including an open search). */
  const select = useCallback(
    (next: Selection) => {
      record({ selection, query }, { selection: next, query: "" });
      setSelection(next);
      setQuery("");
    },
    [record, selection, query],
  );

  const here: Place = { selection, query };
  const back = () => history.back(here);
  const forward = () => history.forward(here);

  // Window listeners are registered once and read the latest handlers through a ref.
  const latest = useRef({ back, forward, select });
  latest.current = { back, forward, select };
  // Stable, so actions that return to the previous place don't change on every render.
  const goBack = useCallback(() => latest.current.back(), []);
  const goForward = useCallback(() => latest.current.forward(), []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const inField = (event.target as HTMLElement).closest(EDITABLE);
      // Esc clears the main pane, unless a field, menu or editor already handled it.
      if (event.key === "Escape") {
        if (!event.defaultPrevented && !inField) latest.current.select(null);
        return;
      }
      if (!event.metaKey || event.altKey || event.ctrlKey || event.shiftKey || inField) return;
      if (event.code === "BracketLeft" || event.key === "ArrowLeft") {
        event.preventDefault();
        latest.current.back();
      } else if (event.code === "BracketRight" || event.key === "ArrowRight") {
        event.preventDefault();
        latest.current.forward();
      }
    };
    const onMouseUp = (event: MouseEvent) => {
      if (event.button === 3) latest.current.back();
      if (event.button === 4) latest.current.forward();
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, []);

  return {
    selection,
    setSelection,
    query,
    setQuery,
    select,
    back: goBack,
    forward: goForward,
    canBack: history.canBack,
    canForward: history.canForward,
  };
}
