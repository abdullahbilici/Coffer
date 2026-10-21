import type { PinKind, PinRef, Tree } from "./api";
import { nodeKey } from "./selection";

/** Same key format as the sidebar tree (`nodeKey`), so pins and tree rows line up. */
export const pinKey = (pin: PinRef) => nodeKey[pin.kind](pin.id);

/** Fast "is this pinned?" lookups for one render. */
export function pinnedSet(tree: Tree): (kind: PinKind, id: number) => boolean {
  const keys = new Set(tree.pins.map(pinKey));
  return (kind, id) => keys.has(pinKey({ kind, id }));
}

/** Pinned entries float to the top; the existing order is kept otherwise (stable sort). */
export function pinnedFirst<T>(list: readonly T[], pinned: (entry: T) => boolean): T[] {
  return [...list].sort((a, b) => Number(pinned(b)) - Number(pinned(a)));
}

export type Drop = { key: string; side: "before" | "after" };

/** The order after moving `dragged` to `drop`, or `null` if nothing would change. */
export function reorder(order: string[], dragged: string, drop: Drop): string[] | null {
  if (dragged === drop.key || !order.includes(dragged)) return null;
  const rest = order.filter((key) => key !== dragged);
  const target = rest.indexOf(drop.key);
  if (target === -1) return null;
  rest.splice(drop.side === "before" ? target : target + 1, 0, dragged);
  return rest.join() === order.join() ? null : rest;
}
