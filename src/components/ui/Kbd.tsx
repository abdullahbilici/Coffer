import { ArrowBigUp, ChevronUp, Command, Option, type LucideIcon } from "lucide-react";
import { cx } from "../../lib/cx";
import type { Modifier } from "../../lib/shortcut";
import styles from "./Kbd.module.css";

const GLYPHS: Record<Modifier, { icon: LucideIcon; name: string }> = {
  Control: { icon: ChevronUp, name: "Control" },
  Alt: { icon: Option, name: "Option" },
  Shift: { icon: ArrowBigUp, name: "Shift" },
  Super: { icon: Command, name: "Command" },
};

type Props = {
  /** The key's label: a letter, a digit or a name like "Space". */
  letter: string;
  modifiers?: readonly Modifier[];
  /** `plain`: just the keys, for a shortcut shown as a value rather than a hint. */
  variant?: "hint" | "plain";
};

/** Shortcut hint, Cmd+`letter` unless told otherwise, with SVG modifier glyphs. */
export function Kbd({ letter, modifiers = ["Super"], variant = "hint" }: Props) {
  const label = [...modifiers.map((m) => GLYPHS[m].name), letter].join(" ");
  return (
    <kbd className={cx(styles.kbd, styles[variant])} aria-label={label}>
      {modifiers.map((m) => {
        const Glyph = GLYPHS[m].icon;
        return <Glyph key={m} size={12} strokeWidth={1.75} />;
      })}
      {letter}
    </kbd>
  );
}
