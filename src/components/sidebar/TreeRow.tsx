import { useState, type MouseEvent, type ReactNode } from "react";
import { ChevronRight, type LucideIcon } from "lucide-react";
import { cx } from "../../lib/cx";
import styles from "./TreeRow.module.css";

const INDENT_PX = 14;

type Props = {
  depth: number;
  icon: LucideIcon;
  label: string;
  /** Omit for leaf rows. */
  expanded?: boolean;
  selected?: boolean;
  onClick?: () => void;
  onToggle?: () => void;
  onContextMenu?: (event: MouseEvent) => void;
  /** Shown on hover, right-aligned. */
  actions?: ReactNode;
  /** Shown when not hovered, right-aligned (counts, badges). */
  trailing?: ReactNode;
};

export function TreeRow({
  depth,
  icon: Icon,
  label,
  expanded,
  selected,
  onClick,
  onToggle,
  onContextMenu,
  actions,
  trailing,
}: Props) {
  const expandable = expanded !== undefined;
  return (
    <div
      role="treeitem"
      aria-expanded={expanded}
      aria-selected={selected}
      className={cx(styles.row, selected && styles.selected)}
      style={{ paddingLeft: 8 + depth * INDENT_PX }}
      onClick={onClick}
      onContextMenu={onContextMenu}
    >
      <span
        className={cx(styles.chevron, expanded && styles.open, !expandable && styles.hidden)}
        onClick={(event) => {
          event.stopPropagation();
          onToggle?.();
        }}
      >
        <ChevronRight size={14} strokeWidth={2} />
      </span>
      <Icon size={16} strokeWidth={1.75} className={styles.icon} />
      <span className={styles.label}>{label}</span>
      {trailing && <span className={styles.trailing}>{trailing}</span>}
      {actions && (
        <span className={styles.actions} onClick={(event) => event.stopPropagation()}>
          {actions}
        </span>
      )}
    </div>
  );
}

type EditRowProps = {
  depth: number;
  icon: LucideIcon;
  initialValue?: string;
  placeholder: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
};

/** Inline name field used for creating and renaming tree nodes. */
export function TreeEditRow({
  depth,
  icon: Icon,
  initialValue = "",
  placeholder,
  onSubmit,
  onCancel,
}: EditRowProps) {
  const [value, setValue] = useState(initialValue);
  const finish = () => {
    const name = value.trim();
    if (name && name !== initialValue) onSubmit(name);
    else onCancel();
  };
  return (
    <div className={cx(styles.row, styles.editing)} style={{ paddingLeft: 8 + depth * INDENT_PX }}>
      <span className={cx(styles.chevron, styles.hidden)} />
      <Icon size={16} strokeWidth={1.75} className={styles.icon} />
      <input
        className={styles.input}
        value={value}
        placeholder={placeholder}
        autoFocus
        spellCheck={false}
        onFocus={(e) => e.target.select()}
        onChange={(e) => setValue(e.target.value)}
        onBlur={finish}
        onKeyDown={(e) => {
          if (e.key === "Enter") finish();
          if (e.key === "Escape") onCancel();
        }}
      />
    </div>
  );
}
