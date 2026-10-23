/**
 * Global shortcuts as the backend stores them: modifiers then a key code joined by `+`, e.g.
 * `Alt+Shift+Space`, in the names the global-shortcut plugin parses (mirrored by
 * `capture::parse`). Keys are physical (`event.code`), so a shortcut means the same keys on
 * every keyboard layout.
 */

/** In the order macOS shows them: Control, Option, Shift, Command. */
export const MODIFIERS = ["Control", "Alt", "Shift", "Super"] as const;

export type Modifier = (typeof MODIFIERS)[number];

const PUNCTUATION: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

const NAMED: Record<string, string> = {
  Space: "Space",
  Enter: "Return",
  Tab: "Tab",
  Backspace: "Delete",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
};

/** How a key code reads on a key cap, or `null` for keys a shortcut can't use. */
export function keyLabel(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  if (/^F([1-9]|1[0-9]|20)$/.test(code)) return code;
  return PUNCTUATION[code] ?? NAMED[code] ?? null;
}

type KeyEventLike = Pick<KeyboardEvent, "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

/**
 * The shortcut a key press makes, or `null` while it's no shortcut yet: only modifiers, a key
 * shortcuts can't use, or nothing but Shift (that would take a key from every app).
 */
export function shortcutFromEvent(event: KeyEventLike): string | null {
  if (keyLabel(event.code) === null) return null;
  const held: Record<Modifier, boolean> = {
    Control: event.ctrlKey,
    Alt: event.altKey,
    Shift: event.shiftKey,
    Super: event.metaKey,
  };
  if (!held.Control && !held.Alt && !held.Super) return null;
  return [...MODIFIERS.filter((m) => held[m]), event.code].join("+");
}

/** A stored shortcut's modifiers (in display order) and its key's label. */
export function shortcutParts(shortcut: string): { modifiers: Modifier[]; key: string } {
  const tokens = shortcut.split("+");
  const code = tokens.at(-1) ?? "";
  const modifiers = MODIFIERS.filter((m) => tokens.slice(0, -1).includes(m));
  return { modifiers, key: keyLabel(code) ?? code };
}
