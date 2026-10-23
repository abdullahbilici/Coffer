import { useState } from "react";
import { Brain, ChevronRight } from "lucide-react";
import type { Copy, Run } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { cx } from "../../lib/cx";
import { AssistantReply } from "./AssistantReply";
import styles from "./Assistant.module.css";

type Props = {
  text: string;
  /** Still streaming in. */
  live?: boolean;
  session: Session;
  run: Run;
  onCopy: Copy;
};

/** The model's reasoning before its words, as a quiet folded line like a tool call. */
export function ThinkingRow({ text, live = false, session, run, onCopy }: Props) {
  const [open, setOpen] = useState(false);
  return (
    <div className={styles.tool}>
      <button
        type="button"
        className={cx(styles.toolHead, live && styles.toolPending)}
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <Brain size={14} strokeWidth={1.75} className={styles.toolIcon} />
        <span className={styles.toolLabel}>Thinking</span>
        <ChevronRight
          size={12}
          strokeWidth={1.75}
          className={cx(styles.toolChevron, open && styles.toolChevronOpen)}
        />
      </button>
      {open && (
        <div className={styles.thinkingBody}>
          <AssistantReply text={text} session={session} run={run} onCopy={onCopy} />
        </div>
      )}
    </div>
  );
}
