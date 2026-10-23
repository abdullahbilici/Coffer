import { useLayoutEffect, useRef, useState, type Ref } from "react";
import { ArrowUp, Square } from "lucide-react";
import { cx } from "../../lib/cx";
import styles from "./Assistant.module.css";

/** Tallest the field grows before it scrolls. */
const MAX_HEIGHT_PX = 200;

type Props = {
  ref: Ref<HTMLTextAreaElement>;
  replying: boolean;
  onSend: (text: string) => Promise<void>;
  onStop: () => void;
};

/** The message field. Enter sends, Shift+Enter adds a line; while replying, Stop. */
export function AssistantComposer({ ref, replying, onSend, onStop }: Props) {
  const [text, setText] = useState("");
  const field = useRef<HTMLTextAreaElement | null>(null);
  const ready = text.trim().length > 0 && !replying;

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
    void onSend(message);
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
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              send();
            }
          }}
        />
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
