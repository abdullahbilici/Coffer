import { asMarker } from "./markup";

/**
 * Remark plugin for Coffer's markdown flavour, used by notes and project overviews:
 * - Inline code is the markup span: a `` `!0!` `` secret marker becomes `<coffer-secret index>`,
 *   any other inline code becomes `<coffer-copy>`. `Markdown` maps both to chips.
 * - Newlines inside paragraphs are line breaks, and runs of spaces are kept.
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

// Every newline is a line break (GitHub-comment style), so notes written line by line keep
// their shape instead of collapsing into one paragraph.
// Runs of spaces are kept (no-break spaces), so "User:   apo" stays aligned.
const keepSpaces = (line: string) => line.replace(/ {2,}/g, (run) => "\u00a0".repeat(run.length));

function splitLines(text: Node): Node[] {
  const lines = (text.value ?? "").split("\n").map(keepSpaces);
  return lines.flatMap((line, i) => [
    ...(i > 0 ? [{ type: "break" }] : []),
    ...(line ? [{ type: "text", value: line }] : []),
  ]);
}

function transform(node: Node) {
  if (!node.children) return;
  node.children = node.children.flatMap((child) => {
    if (child.type === "text") return splitLines(child);
    if (child.type !== "inlineCode") {
      transform(child);
      return [child];
    }
    const index = asMarker(child.value ?? "");
    child.data =
      index === null
        ? { hName: COPY_TAG }
        : { hName: SECRET_TAG, hProperties: { index }, hChildren: [] };
    return [child];
  });
}

export function remarkMarkup() {
  return (tree: Node) => transform(tree);
}
