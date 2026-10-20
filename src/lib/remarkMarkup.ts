/**
 * Remark plugin that turns Coffer markup in markdown into custom elements:
 * inline code becomes `<coffer-copy>`, and `{{0}}` secret markers in the stored template become
 * `<coffer-secret index="0">`. `Markdown` maps both to the same chips used in item bodies.
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

const MARKER = /\{\{(\d+)\}\}/g;

function splitMarkers(text: string): Node[] {
  const nodes: Node[] = [];
  let last = 0;
  for (const match of text.matchAll(MARKER)) {
    if (match.index > last) nodes.push({ type: "text", value: text.slice(last, match.index) });
    nodes.push({
      type: "cofferSecret",
      data: { hName: SECRET_TAG, hProperties: { index: Number(match[1]) }, hChildren: [] },
    });
    last = match.index + match[0].length;
  }
  if (last < text.length) nodes.push({ type: "text", value: text.slice(last) });
  return nodes;
}

function transform(node: Node) {
  if (!node.children) return;
  node.children = node.children.flatMap((child) => {
    if (child.type === "inlineCode") return [{ ...child, data: { hName: COPY_TAG } }];
    if (child.type === "text" && child.value) return splitMarkers(child.value);
    transform(child);
    return [child];
  });
}

export function remarkMarkup() {
  return (tree: Node) => transform(tree);
}
