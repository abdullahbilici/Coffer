import { useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  Box,
  Boxes,
  Calculator,
  ChevronRight,
  ClipboardList,
  FilePen,
  FileText,
  Folder,
  FolderPlus,
  ListTodo,
  Search,
  Sparkles,
  Wallet,
  type LucideIcon,
} from "lucide-react";
import type { Copy, Run, VaultActions } from "../../app/actions";
import type { Thread } from "../../hooks/useAssistant";
import type { Session } from "../../hooks/useSession";
import type { ToolCall, Tree } from "../../lib/api";
import {
  replySources,
  replyStatus,
  threadBlocks,
  toolLabel,
  type Block,
} from "../../lib/assistant";
import { cx } from "../../lib/cx";
import { AssistantReply } from "./AssistantReply";
import { DraftCard } from "./DraftCard";
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
  create_project: Box,
  create_folder: FolderPlus,
  create_note: FilePen,
  add_notes: FilePen,
  add_field: FilePen,
  update_field: FilePen,
  remove_field: FilePen,
  rename: FilePen,
  move_note: Folder,
  delete_note: FilePen,
  create_todo: ListTodo,
  update_todo: ListTodo,
  complete_todo: ListTodo,
  create_payment: Wallet,
  update_payment: Wallet,
  cancel_payment: Wallet,
};

/** Within this distance of the bottom, new content keeps the thread scrolled to the end. */
const STICK_PX = 48;

type Shared = { tree: Tree; session: Session; run: Run; onCopy: Copy; actions: VaultActions };

type Drafts = {
  onApply: (messageId: number) => Promise<void>;
  onDiscard: (messageId: number) => Promise<void>;
};

type Props = Shared & Drafts & { thread: Thread; replying: boolean };

/** The messages of one conversation, with the reply streaming in at the end. */
export function AssistantThread({ thread, replying, onApply, onDiscard, ...shared }: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  const results = useMemo(() => {
    const map = new Map<string, string>();
    for (const m of thread.messages) if (m.role === "tool") map.set(m.callId, m.result);
    return map;
  }, [thread.messages]);

  const status = replying ? replyStatus(thread.messages, thread.phase, thread.streaming) : null;

  // Follows the reply while the user is at the bottom; scrolling up stops following.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [thread.messages, thread.streaming, thread.error, replying, status]);

  useLayoutEffect(() => {
    stick.current = true;
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [thread.opened]);

  const blocks = useMemo(() => threadBlocks(thread.messages), [thread.messages]);

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
              onApply={onApply}
              onDiscard={onDiscard}
              {...shared}
            />
          ))}
          {thread.streaming && (
            <AssistantReply
              text={thread.streaming}
              session={shared.session}
              run={shared.run}
              onCopy={shared.onCopy}
            />
          )}
          {status && (
            <p key={status} className={styles.replyStatus} role="status">
              {status}
            </p>
          )}
          {thread.error && <p className={styles.error}>{thread.error}</p>}
        </div>
      )}
    </div>
  );
}

type ThreadBlockProps = Shared &
  Drafts & {
    block: Block;
    results: ReadonlyMap<string, string>;
    replying: boolean;
  };

function ThreadBlock({
  block,
  results,
  replying,
  onApply,
  onDiscard,
  ...shared
}: ThreadBlockProps) {
  switch (block.kind) {
    case "user":
      return (
        <>
          {block.paste !== null && <Paste text={block.paste} />}
          {block.text && <div className={styles.user}>{block.text}</div>}
        </>
      );
    case "draft":
      return (
        <DraftCard
          groups={block.groups}
          status={block.status}
          session={shared.session}
          run={shared.run}
          onCopy={shared.onCopy}
          onApply={() => onApply(block.id)}
          onDiscard={() => onDiscard(block.id)}
        />
      );
    case "reply": {
      const { tree, actions } = shared;
      const list = replySources(block.text, block.read, tree);
      return (
        <AssistantReply
          text={block.text}
          session={shared.session}
          run={shared.run}
          onCopy={shared.onCopy}
          sources={{ list, tree, actions }}
        />
      );
    }
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

/** Pasted text sent for import, as a quiet attachment: its values stay out of the thread. */
function Paste({ text }: { text: string }) {
  const lines = text.split("\n").filter((l) => l.trim()).length;
  return (
    <div className={styles.paste}>
      <ClipboardList size={14} strokeWidth={1.75} className={styles.pasteIcon} />
      <span>Pasted text</span>
      <span className={styles.pasteMeta}>{lines === 1 ? "1 line" : `${lines} lines`}</span>
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
        <span className={styles.toolLabel}>{toolLabel(call, tree, result)}</span>
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
