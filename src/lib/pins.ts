import type { PinKind, PinRef, Tree } from "./api";

export const pinKey = (pin: PinRef) => `${pin.kind}:${pin.id}`;

/** Fast "is this pinned?" lookups for one render. */
export function pinnedSet(tree: Tree): (kind: PinKind, id: number) => boolean {
  const keys = new Set(tree.pins.map(pinKey));
  return (kind, id) => keys.has(pinKey({ kind, id }));
}
