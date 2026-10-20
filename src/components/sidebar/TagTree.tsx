import { Tag } from "lucide-react";
import { nodeKey, type Selection } from "../../lib/selection";
import type { TagNode } from "../../lib/tags";
import { TreeRow } from "./TreeRow";

type Props = {
  nodes: TagNode[];
  expanded: ReadonlySet<string>;
  selection: Selection;
  onToggle: (key: string) => void;
  onExpand: (keys: string[]) => void;
  onSelect: (path: string) => void;
};

export function TagTree(props: Props) {
  return (
    <div role="tree">
      {props.nodes.map((node) => (
        <TagBranch key={node.path} node={node} depth={0} {...props} />
      ))}
    </div>
  );
}

function TagBranch({ node, depth, ...props }: Props & { node: TagNode; depth: number }) {
  const { expanded, selection, onToggle, onExpand, onSelect } = props;
  const key = nodeKey.tag(node.path);
  const hasChildren = node.children.length > 0;
  const isOpen = expanded.has(key);
  const selected = selection?.kind === "tag" && selection.path === node.path;

  return (
    <>
      <TreeRow
        depth={depth}
        icon={Tag}
        label={node.name}
        expanded={hasChildren ? isOpen : undefined}
        selected={selected}
        trailing={node.count}
        onClick={() => {
          // Same as the project tree: first click selects and opens, the next one toggles.
          if (selected) {
            if (hasChildren) onToggle(key);
          } else {
            onSelect(node.path);
            if (hasChildren) onExpand([key]);
          }
        }}
        onToggle={() => onToggle(key)}
      />
      {isOpen &&
        node.children.map((child) => (
          <TagBranch key={child.path} node={child} depth={depth + 1} {...props} />
        ))}
    </>
  );
}
