import { useLayoutEffect, useRef, useState, type Ref } from "react";
import { ArrowUp, ClipboardList, Square, X } from "lucide-react";
import { cx } from "../../lib/cx";
import { IconButton } from "../ui/IconButton";
import styles from "./Assistant.module.css";

/** Tallest the field grows before it scrolls. */
const MAX_HEIGHT_PX = 200;

/** A paste this long is a dump to import, not part of the message. */
const PASTE_LINES = 6;
const PASTE_CHARS = 600;

type Props = {
  ref: Ref<HTMLTextAreaElement>;
  replying: boolean;
  onSend: (text: string, paste: string | null) => Promise<void>;
  onStop: () => void;
};

/**
 * The message field. Enter sends, Shift+Enter adds a line; while replying, Stop. A long paste
 * becomes an attachment that is imported with the message.
 */
export function AssistantComposer({ ref, replying, onSend, onStop }: Props) {
  const [text, setText] = useState("");
  const [paste, setPaste] = useState<string | null>(null);
  const field = useRef<HTMLTextAreaElement | null>(null);
  const ready = (text.trim().length > 0 || paste !== null) && !replying;
  const pasteLines = paste?.split("\n").filter((l) => l.trim()).length ?? 0;

  // Grows with its content up to MAX_HEIGHT_PX.
  useLayoutEffect(() => {
    const el = field.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT_PX)}px`;
  }, [text]);

  const send = () => {
    if (!ready) return;
    const message = text;
    setText("");
    setPaste(null);
    void onSend(message, paste);
  };

  return (
    <div className={styles.composerWrap}>
      <form
        className={styles.composer}
        onSubmit={(event) => {
          event.preventDefault();
          send();
        }}
      >
        <div className={styles.composerColumn}>
          {paste !== null && (
            <div className={styles.attachment}>
              <ClipboardList size={14} strokeWidth={1.75} className={styles.pasteIcon} />
              <span className={styles.attachmentLabel}>
                Pasted text{" "}
                <span className={styles.pasteMeta}>
                  {pasteLines === 1 ? "1 line" : `${pasteLines} lines`}
                </span>
              </span>
              <IconButton icon={X} label="Remove" size="sm" onClick={() => setPaste(null)} />
            </div>
          )}
          <textarea
            ref={(el) => {
              field.current = el;
              if (typeof ref === "function") ref(el);
              else if (ref) ref.current = el;
            }}
            className={styles.field}
            rows={1}
            value={text}
            placeholder="Message"
            autoFocus
            onChange={(e) => setText(e.target.value)}
            onPaste={(e) => {
              const pasted = e.clipboardData.getData("text/plain");
              const lines = pasted.split("\n").filter((l) => l.trim()).length;
              if (lines < PASTE_LINES && pasted.length < PASTE_CHARS) return;
              e.preventDefault();
              setPaste((prev) => (prev ? `${prev}\n\n${pasted}` : pasted));
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                send();
              }
              if (e.key === "Backspace" && !text && paste !== null) setPaste(null);
            }}
          />
        </div>
        {replying ? (
          <button type="button" className={styles.send} aria-label="Stop" onClick={onStop}>
            <Square size={12} strokeWidth={1.75} className={styles.stopIcon} />
          </button>
        ) : (
          <button
            type="submit"
            className={cx(styles.send, ready && styles.sendReady)}
            aria-label="Send"
            disabled={!ready}
          >
            <ArrowUp size={16} strokeWidth={1.75} />
          </button>
        )}
      </form>
    </div>
  );
}
