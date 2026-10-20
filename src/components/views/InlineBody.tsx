import type { Segment } from "../../lib/markup";
import { CopyChip } from "./CopyChip";
import { SecretChip } from "./SecretChip";
import styles from "./InlineBody.module.css";

type Props = {
  segments: Segment[];
  /** Indices of secrets currently unmasked. */
  shown: ReadonlySet<number>;
  onToggleSecret: (index: number) => void;
  onCopySecret: (index: number) => void;
  onCopyText: (text: string) => void;
};

/** Plain-text item body with copyable chips and masked secrets. */
export function InlineBody({ segments, shown, onToggleSecret, onCopySecret, onCopyText }: Props) {
  return (
    <pre className={styles.body}>
      {segments.map((segment, i) => {
        if (segment.kind === "text") return segment.text;
        if (segment.kind === "copy") {
          return <CopyChip key={`copy:${i}`} text={segment.text} onCopy={onCopyText} />;
        }
        const { index, text } = segment;
        return (
          <SecretChip
            key={`secret:${index}`}
            value={text}
            shown={shown.has(index)}
            onToggle={() => onToggleSecret(index)}
            onCopy={() => onCopySecret(index)}
          />
        );
      })}
    </pre>
  );
}
