import { useCallback, useReducer, useRef } from "react";
import type { Selection } from "../lib/selection";

/** Where the user was: what the main pane showed, and the search that was open. */
export type Place = { selection: Selection; query: string };

const MAX_ENTRIES = 100;

const samePlace = (a: Place, b: Place) =>
  a.query === b.query && JSON.stringify(a.selection) === JSON.stringify(b.selection);

/** Editors and the blank start screen aren't places worth going back to. */
const keep = (place: Place) =>
  place.selection?.kind !== "newItem" && (place.selection !== null || place.query !== "");

/**
 * Back/forward history like Safari or Finder. `record` is called on every navigation;
 * stale entries (deleted items, folders, projects) are skipped when stepping.
 */
export function useHistory(apply: (place: Place) => void, isValid: (place: Place) => boolean) {
  const past = useRef<Place[]>([]);
  const future = useRef<Place[]>([]);
  const [, rerender] = useReducer((n: number) => n + 1, 0);

  const record = useCallback((from: Place, to: Place) => {
    if (samePlace(from, to)) return;
    if (keep(from)) past.current = [...past.current, from].slice(-MAX_ENTRIES);
    future.current = [];
    rerender();
  }, []);

  const step = (source: { current: Place[] }, target: { current: Place[] }, current: Place) => {
    while (source.current.length > 0) {
      const place = source.current[source.current.length - 1];
      source.current = source.current.slice(0, -1);
      if (!isValid(place)) continue;
      if (keep(current)) target.current = [...target.current, current];
      apply(place);
      break;
    }
    rerender();
  };

  return {
    record,
    back: (current: Place) => step(past, future, current),
    forward: (current: Place) => step(future, past, current),
    canBack: past.current.length > 0,
    canForward: future.current.length > 0,
  };
}
