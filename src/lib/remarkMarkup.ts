import { asMarker } from "./markup";

/**
 * Remark plugin that turns Coffer markup in markdown into custom elements. Inline code is
 * already the markup's span, so: a `` `!0!` `` secret marker becomes `<coffer-secret index>`,
 * any other inline code becomes `<coffer-copy>`. `Markdown` maps both to the item-body chips.
 */

/** The few mdast fields this plugin touches. */
type Node = {
  type: string;
  value?: string;
  children?: Node[];
  data?: Record<string, unknown>;
};

export const COPY_TAG = "coffer-copy";
export const SECRET_TAG = "coffer-secret";

function transform(node: Node) {
  for (const child of node.children ?? []) {
    if (child.type !== "inlineCode") {
      transform(child);
      continue;
    }
    const index = asMarker(child.value ?? "");
    child.data =
      index === null
        ? { hName: COPY_TAG }
        : { hName: SECRET_TAG, hProperties: { index }, hChildren: [] };
  }
}

export function remarkMarkup() {
  return (tree: Node) => transform(tree);
}
