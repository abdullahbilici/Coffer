import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppWindow, Check, Sparkles } from "lucide-react";
import { IconButton } from "../components/ui/IconButton";
import { AssistantReply } from "../components/views/AssistantReply";
import { DraftCard } from "../components/views/DraftCard";
import { useAssistant, warmUp } from "../hooks/useAssistant";
import { useSession } from "../hooks/useSession";
import { useToast } from "../hooks/useToast";
import { api, errorMessage } from "../lib/api";
import { threadBlocks } from "../lib/assistant";
import { cx } from "../lib/cx";
import { motion } from "../lib/motion";
import type { Copy, Run } from "./actions";
import styles from "./Capture.module.css";

/** Mirror `capture::SHOWN` and `capture::DISMISS`. */
const SHOWN = "capture-shown";
const DISMISS = "capture-dismiss";

/** The window is the panel plus room for its shadow: `.stage` padding, top and bottom. */
const MARGIN_Y_PX = 16 + 32;

/** How long "Applied" shows before the panel goes away by itself. */
const APPLIED_MS = 900;

const reloadNothing = () => Promise.resolve();

/**
 * Quick capture, the second window: one field whose text goes to the assistant as a new
 * conversation, and the reply under it, a draft to apply or an answer. Same session, assistant
 * and draft card as the main window; nothing is shared but the backend.
 *
 * The window is created once and only shown and hidden; the panel plays its entrance on every
 * show and its exit before every hide. Settled results (answered, applied, discarded, or opened
 * in Coffer) are cleared the next time it shows; a reply still running or a draft still waiting
 * is kept.
 */
export function Capture() {
  const session = useSession();
  const toast = useToast();
  const [error, setError] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [shown, setShown] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  /** While Touch ID may be up, losing the keyboard doesn't send the panel away. */
  const holding = useRef(0);
  const shownRef = useRef(false);
  const settledAtHide = useRef(false);

  const run: Run = useCallback(async (action) => {
    try {
      await action();
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  const { show: showToast } = toast;
  const copy: Copy = useCallback(
    async (value, secret = false) => {
      if (secret) await api.copySecret(value);
      else await navigator.clipboard.writeText(value);
      showToast("Copied", "success");
    },
    [showToast],
  );

  const assistant = useAssistant(session, run, reloadNothing);
  const { thread, replying } = assistant;
  const blocks = useMemo(
    () => threadBlocks(thread.messages).filter((b) => b.kind !== "user" && b.kind !== "tools"),
    [thread.messages],
  );
  const pendingDraft = blocks.some((b) => b.kind === "draft" && b.status === "pending");
  const settled = thread.id !== null && !replying && !pendingDraft;
  const settledRef = useRef(settled);
  settledRef.current = settled;

  /** Runs `action` without letting a Touch ID prompt send the panel away. */
  const held = useCallback(async (action: () => Promise<void>) => {
    holding.current += 1;
    try {
      await action();
    } finally {
      holding.current -= 1;
      void api.focusCapture();
    }
  }, []);

  const dismiss = useCallback(() => {
    if (!shownRef.current) return;
    shownRef.current = false;
    settledAtHide.current = settledRef.current;
    setShown(false);
    setTimeout(() => void api.hideCapture(), motion().fastMs);
  }, []);

  const { open } = assistant;
  useEffect(() => {
    const onShown = () => {
      if (!shownRef.current) {
        shownRef.current = true;
        warmUp();
        if (settledAtHide.current) {
          settledAtHide.current = false;
          void open(null);
          setText("");
          setError(null);
        }
        setShown(true);
        // The other window may have locked or unlocked meanwhile.
        void api.sessionInfo().then(session.update, () => {});
      }
      requestAnimationFrame(() => field.current?.select());
    };
    const unlistenShown = listen(SHOWN, onShown);
    const unlistenDismiss = listen(DISMISS, dismiss);
    const unlistenFocus = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (!focused && holding.current === 0) dismiss();
    });
    return () => {
      for (const unlisten of [unlistenShown, unlistenDismiss, unlistenFocus]) {
        void unlisten.then((stop) => stop());
      }
    };
  }, [open, dismiss, session.update]);

  // The window follows the panel's height, so nothing around it takes clicks.
  useLayoutEffect(() => {
    const el = panel.current;
    if (!el) return;
    const observer = new ResizeObserver(() => void api.fitCapture(el.offsetHeight + MARGIN_Y_PX));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const send = () => {
    const message = text.trim();
    if (!message || replying) return;
    setError(null);
    void open(null);
    void held(() => assistant.send(message));
  };

  const openInCoffer = () => {
    const id = thread.id;
    if (id === null) return;
    settledAtHide.current = true;
    shownRef.current = false;
    setShown(false);
    void run(() => api.openInCoffer(id));
  };

  const { applyDraft, discardDraft } = assistant;
  const apply = (messageId: number) =>
    held(async () => {
      await applyDraft(messageId);
      setTimeout(() => {
        if (settledRef.current) dismiss();
      }, APPLIED_MS);
    });

  return (
    <div
      className={styles.stage}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) dismiss();
      }}
    >
      <div ref={panel} className={cx(styles.panel, shown && styles.shown)}>
        <form
          className={styles.bar}
          onSubmit={(e) => {
            e.preventDefault();
            send();
          }}
        >
          <Sparkles size={20} strokeWidth={1.75} className={styles.mark} />
          <input
            ref={field}
            className={styles.field}
            value={text}
            placeholder="Capture"
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Escape") {
                e.preventDefault();
                dismiss();
              } else if (e.metaKey && e.key.toLowerCase() === "o") {
                e.preventDefault();
                openInCoffer();
              }
            }}
          />
          <div className={styles.status}>
            {toast.toast ? (
              <span
                key={toast.toast.id}
                className={cx(styles.copied, toast.toast.leaving && styles.leaving)}
              >
                <Check size={14} strokeWidth={1.75} />
                {toast.toast.message}
              </span>
            ) : replying ? (
              <span className={styles.thinking} aria-label="Thinking">
                <span />
                <span />
                <span />
              </span>
            ) : (
              thread.id !== null && (
                <IconButton icon={AppWindow} label="Open in Coffer" onClick={openInCoffer} />
              )
            )}
          </div>
        </form>
        {(blocks.length > 0 || thread.streaming || thread.error || error) && (
          <div key={thread.opened} className={styles.result}>
            {blocks.map((block) => {
              switch (block.kind) {
                case "reply":
                  return (
                    <div key={block.key} className={styles.answer}>
                      <AssistantReply text={block.text} session={session} run={run} onCopy={copy} />
                    </div>
                  );
                case "draft":
                  return (
                    <DraftCard
                      key={block.key}
                      compact
                      groups={block.groups}
                      status={block.status}
                      session={session}
                      run={run}
                      onCopy={copy}
                      onApply={() => apply(block.id)}
                      onDiscard={() => discardDraft(block.id)}
                    />
                  );
                case "stopped":
                  return (
                    <p key={block.key} className={styles.quiet}>
                      Stopped
                    </p>
                  );
                default:
                  return null;
              }
            })}
            {thread.streaming && (
              <div className={styles.answer}>
                <AssistantReply text={thread.streaming} session={session} run={run} onCopy={copy} />
              </div>
            )}
            {(thread.error || error) && <p className={styles.error}>{thread.error ?? error}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
