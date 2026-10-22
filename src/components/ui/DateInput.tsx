import { X } from "lucide-react";
import { IconButton } from "./IconButton";
import { Input } from "./Input";
import styles from "./DateInput.module.css";

type Props = {
  /** `YYYY-MM-DD`, or `null` for no date. */
  value: string | null;
  onChange: (value: string | null) => void;
  /** Off for required dates. */
  clearable?: boolean;
};

/** Native date picker, plus a clear button since WebKit's has none. */
export function DateInput({ value, onChange, clearable = true }: Props) {
  return (
    <div className={styles.root}>
      <Input
        type="date"
        className={styles.input}
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value || null)}
      />
      {clearable && value && (
        <IconButton icon={X} label="Clear date" size="sm" onClick={() => onChange(null)} />
      )}
    </div>
  );
}
