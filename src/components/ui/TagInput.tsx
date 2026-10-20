import { useId, useState, type KeyboardEvent } from "react";
import { X } from "lucide-react";
import styles from "./TagInput.module.css";

type Props = {
  value: string[];
  onChange: (tags: string[]) => void;
  /** Existing tags offered as autocomplete. */
  suggestions: string[];
};

/** Chip input. Enter, comma or Tab commits a tag; Backspace on empty removes the last one. */
export function TagInput({ value, onChange, suggestions }: Props) {
  const [draft, setDraft] = useState("");
  const listId = useId();

  const commit = () => {
    const tag = draft.trim();
    if (tag && !value.includes(tag)) onChange([...value, tag]);
    setDraft("");
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if ((event.key === "Enter" || event.key === "," || event.key === "Tab") && draft.trim()) {
      event.preventDefault();
      commit();
    } else if (event.key === "Backspace" && !draft && value.length > 0) {
      onChange(value.slice(0, -1));
    }
  };

  return (
    <div className={styles.root}>
      {value.map((tag) => (
        <span key={tag} className={styles.chip}>
          {tag}
          <button
            type="button"
            className={styles.remove}
            aria-label={`Remove ${tag}`}
            onClick={() => onChange(value.filter((t) => t !== tag))}
          >
            <X size={12} strokeWidth={2} />
          </button>
        </span>
      ))}
      <input
        className={styles.input}
        value={draft}
        list={listId}
        placeholder={value.length === 0 ? "Add tags" : ""}
        spellCheck={false}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={onKeyDown}
        onBlur={commit}
      />
      <datalist id={listId}>
        {suggestions
          .filter((s) => !value.includes(s))
          .map((s) => (
            <option key={s} value={s} />
          ))}
      </datalist>
    </div>
  );
}
