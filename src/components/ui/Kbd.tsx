import { Command } from "lucide-react";
import styles from "./Kbd.module.css";

/** Shortcut hint for Cmd+`letter`, drawn with an SVG command glyph. */
export function Kbd({ letter }: { letter: string }) {
  return (
    <kbd className={styles.kbd} aria-label={`Command ${letter}`}>
      <Command size={10} strokeWidth={2} />
      {letter}
    </kbd>
  );
}
