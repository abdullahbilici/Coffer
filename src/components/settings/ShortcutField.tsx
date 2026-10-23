import { useState } from "react";
import { X } from "lucide-react";
import { cx } from "../../lib/cx";
import { shortcutFromEvent, shortcutParts } from "../../lib/shortcut";
import { IconButton } from "../ui/IconButton";
import { Kbd } from "../ui/Kbd";
import styles from "./ShortcutField.module.css";

type Props = {
  label: string;
  /** `null`: off. */
  value: string | null;
  onChange: (shortcut: string | null) => void;
};

/**
 * A global shortcut. Click, then press the keys; Esc or clicking away keeps the old one,
 * Delete turns it off, and so does the button inside the field.
 */
export function ShortcutField({ label, value, onChange }: Props) {
  const [recording, setRecording] = useState(false);
  const parts = value === null ? null : shortcutParts(value);

  return (
    <div className={cx(styles.field, recording && styles.recording)}>
      <button
        type="button"
        aria-label={label}
        className={styles.keys}
        onClick={() => setRecording(true)}
        onBlur={() => setRecording(false)}
        onKeyDown={(e) => {
          if (!recording) return;
          e.preventDefault();
          e.stopPropagation();
          const plain = !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
          if (e.key === "Escape" && plain) {
            setRecording(false);
          } else if (e.key === "Backspace" && plain) {
            setRecording(false);
            onChange(null);
          } else {
            const shortcut = shortcutFromEvent(e.nativeEvent);
            if (shortcut === null) return;
            setRecording(false);
            if (shortcut !== value) onChange(shortcut);
          }
        }}
      >
        {recording ? (
          <span className={styles.placeholder}>Type shortcut</span>
        ) : parts ? (
          <Kbd letter={parts.key} modifiers={parts.modifiers} variant="plain" />
        ) : (
          <span className={styles.placeholder}>Off</span>
        )}
      </button>
      {value !== null && !recording && (
        <IconButton
          icon={X}
          label="Turn off"
          size="sm"
          className={styles.clear}
          onClick={() => onChange(null)}
        />
      )}
    </div>
  );
}
