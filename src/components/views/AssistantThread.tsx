import { useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  Box,
  Boxes,
  Calculator,
  ChevronRight,
  FileText,
  Folder,
  ListTodo,
  Search,
  Sparkles,
  Wallet,
  type LucideIcon,
} from "lucide-react";
import type { Copy, Run } from "../../app/actions";
import type { Thread } from "../../hooks/useAssistant";
import { useHandleChips } from "../../hooks/useHandleChips";
import type { Session } from "../../hooks/useSession";
import type { ToolCall, Tree } from "../../lib/api";
import { threadBlocks, toolLabel, withSecretChips, type Block } from "../../lib/assistant";
import { cx } from "../../lib/cx";
import { Markdown } from "./Markdown";
import styles from "./Assistant.module.css";

const TOOL_ICONS: Record<string, LucideIcon> = {
  list_projects: Boxes,
  get_project: Box,
  list_folder: Folder,
  search: Search,
  get_note: FileText,
  list_todos: ListTodo,
  list_payments: Wallet,
  project_spend: Calculator,
};

/** Within this distance of the bottom, new content keeps the thread scrolled to the end. */
const STICK_PX = 48;

type Shared = { tree: Tree; session: Session; run: Run; onCopy: Copy };

type Props = Shared & { thread: Thread; replying: boolean };

/** The messages of one conversation, with the reply streaming in at the end. */
export function AssistantThread({ thread, replying, ...shared }: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  const results = useMemo(() => {
    const map = new Map<string, string>();
    for (const m of thread.messages) if (m.role === "tool") map.set(m.callId, m.result);
    return map;
  }, [thread.messages]);

  // Follows the reply while the user is at the bottom; scrolling up stops following.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [thread.messages, thread.streaming, thread.error, replying]);

  useLayoutEffect(() => {
    stick.current = true;
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [thread.opened]);

  const blocks = useMemo(() => threadBlocks(thread.messages), [thread.messages]);
  const last = thread.messages.at(-1);
  const waiting = replying && !thread.streaming && (last?.role === "user" || last?.role === "tool");

  return (
    <div
      ref={scroller}
      className={styles.scroll}
      onScroll={(e) => {
        const el = e.currentTarget;
        stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_PX;
      }}
    >
      {thread.messages.length === 0 ? (
        <p key={thread.opened} className={styles.emptyLabel}>
          Ask about your vault
        </p>
      ) : (
        <div key={thread.opened} className={styles.messages}>
          {blocks.map((block) => (
            <ThreadBlock
              key={block.key}
              block={block}
              results={results}
              replying={replying}
              {...shared}
            />
          ))}
          {thread.streaming && <Reply text={thread.streaming} {...shared} />}
          {waiting && (
            <span className={styles.thinking} aria-label="Thinking">
              <span />
              <span />
              <span />
            </span>
          )}
          {thread.error && <p className={styles.error}>{thread.error}</p>}
        </div>
      )}
    </div>
  );
}

type ThreadBlockProps = Shared & {
  block: Block;
  results: ReadonlyMap<string, string>;
  replying: boolean;
};

function ThreadBlock({ block, results, replying, ...shared }: ThreadBlockProps) {
  switch (block.kind) {
    case "user":
      return <div className={styles.user}>{block.text}</div>;
    case "reply":
      return <Reply text={block.text} {...shared} />;
    case "tools":
      return (
        <div className={styles.tools}>
          {block.calls.map((call) => (
            <ToolRow
              key={call.id}
              call={call}
              result={results.get(call.id)}
              pending={replying}
              tree={shared.tree}
            />
          ))}
        </div>
      );
    case "stopped":
      return <p className={styles.stopped}>Stopped</p>;
  }
}

/** Assistant markdown; secret handles render as masked chips the user can reveal. */
function Reply({ text, session, run, onCopy }: Shared & { text: string }) {
  const { source, handles } = useMemo(() => withSecretChips(text), [text]);
  const chips = useHandleChips(handles, session, run, onCopy);
  return (
    <div className={styles.reply}>
      <Markdown source={source} chips={chips} size="md" />
    </div>
  );
}

type ToolRowProps = {
  call: ToolCall;
  result: string | undefined;
  /** The reply is still running, so a missing result is on its way. */
  pending: boolean;
  tree: Tree;
};

/** One tool call as a quiet line; opens to show what the tool returned. */
function ToolRow({ call, result, pending, tree }: ToolRowProps) {
  const [open, setOpen] = useState(false);
  const Icon = TOOL_ICONS[call.name] ?? Sparkles;
  const done = result !== undefined;
  return (
    <div className={styles.tool}>
      <button
        type="button"
        className={cx(styles.toolHead, !done && pending && styles.toolPending)}
        disabled={!done}
        aria-expanded={done ? open : undefined}
        onClick={() => setOpen((o) => !o)}
      >
        <Icon size={14} strokeWidth={1.75} className={styles.toolIcon} />
        <span className={styles.toolLabel}>{toolLabel(call, tree)}</span>
        {done && (
          <ChevronRight
            size={12}
            strokeWidth={1.75}
            className={cx(styles.toolChevron, open && styles.toolChevronOpen)}
          />
        )}
      </button>
      {open && done && <pre className={styles.toolResult}>{result}</pre>}
    </div>
  );
}
