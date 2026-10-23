import { useEffect, type RefObject } from "react";
import { Fingerprint, SquarePen } from "lucide-react";
import type { Copy, Run, VaultActions } from "../../app/actions";
import type { Assistant } from "../../hooks/useAssistant";
import type { Session } from "../../hooks/useSession";
import type { Tree } from "../../lib/api";
import { Button } from "../ui/Button";
import { IconButton } from "../ui/IconButton";
import { AssistantComposer } from "./AssistantComposer";
import { AssistantThread } from "./AssistantThread";
import { ConversationList } from "./ConversationList";
import styles from "./Assistant.module.css";

type Props = {
  assistant: Assistant;
  tree: Tree;
  session: Session;
  run: Run;
  onCopy: Copy;
  actions: VaultActions;
  composerRef: RefObject<HTMLTextAreaElement | null>;
};

/** Conversations on the left; the open one with its composer on the right. */
export function AssistantView({
  assistant,
  tree,
  session,
  run,
  onCopy,
  actions,
  composerRef,
}: Props) {
  const { conversations, thread, load } = assistant;

  // Opening the view loads history, asking for Touch ID if the window is closed. Once loaded,
  // a closing window drops it and the locked state below offers to unlock again.
  useEffect(() => {
    if (conversations === null) void load();
    // Only when the view opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const title =
    thread.id === null ? "New chat" : (conversations?.find((c) => c.id === thread.id)?.title ?? "");

  return (
    <div className={styles.root}>
      <aside className={styles.side}>
        <div className={styles.sideBar} data-tauri-drag-region>
          <IconButton
            icon={SquarePen}
            label="New chat"
            disabled={conversations === null}
            onClick={() => {
              void assistant.open(null);
              composerRef.current?.focus();
            }}
          />
        </div>
        {conversations && (
          <ConversationList
            conversations={conversations}
            selectedId={thread.id}
            onOpen={(id) => void assistant.open(id)}
            onRename={(id, name) => void assistant.rename(id, name)}
            onDelete={assistant.remove}
          />
        )}
      </aside>
      <section className={styles.main}>
        <header className={styles.titlebar} data-tauri-drag-region>
          {conversations && (
            <h1 key={thread.id ?? "new"} className={styles.title} data-tauri-drag-region>
              {title}
            </h1>
          )}
        </header>
        {conversations === null ? (
          !session.unlocked && (
            <div className={styles.locked}>
              <span className={styles.lockedLabel}>Locked</span>
              <Button size="sm" icon={Fingerprint} onClick={() => void load()}>
                Unlock
              </Button>
            </div>
          )
        ) : (
          <>
            <AssistantThread
              thread={thread}
              replying={assistant.replying}
              tree={tree}
              session={session}
              run={run}
              onCopy={onCopy}
              actions={actions}
              onApply={assistant.applyDraft}
              onDiscard={assistant.discardDraft}
            />
            <AssistantComposer
              ref={composerRef}
              replying={assistant.replying}
              onSend={assistant.send}
              onStop={assistant.cancel}
            />
          </>
        )}
      </section>
    </div>
  );
}
