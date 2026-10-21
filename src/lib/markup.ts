/**
 * Inline markup in item bodies and project overviews. Mirrors `src-tauri/src/secrets.rs`.
 * - `` `value` `` is a copyable value; not secret.
 * - `` `!value!` `` is a secret. The backtick and `!` must touch on both sides; `` ` !x! ` `` is
 *   just a copyable value. The backend stores secrets encrypted and leaves `` `!0!` ``-style
 *   markers in the readable template.
 */

export type Span = { open: string; close: string };

export const COPYABLE: Span = { open: "`", close: "`" };
export const SECRET: Span = { open: "`!", close: "!`" };

const SPAN = /`([^`\n]+)`/g;
const MARKER = /^!(\d+)!$/;

/** The secret inside a span's content (`!value!`), or `null` when it is a copyable value. */
export function asSecret(content: string): string | null {
  return content.length > 2 && content.startsWith("!") && content.endsWith("!")
    ? content.slice(1, -1)
    : null;
}

/** The index of a stored `` `!n!` `` marker, or `null`. */
export function asMarker(content: string): number | null {
  const match = MARKER.exec(content);
  return match ? Number(match[1]) : null;
}

/** Secret values in order, as the backend numbers them (`` `!0!` ``, `` `!1!` ``, …). */
export function secretValues(body: string): string[] {
  return [...body.matchAll(SPAN)].flatMap((match) => {
    const secret = asSecret(match[1]);
    return secret === null ? [] : [secret];
  });
}

/** The body without markup, for "Copy all". */
export function toPlainText(body: string): string {
  return body.replace(SPAN, (_, content: string) => asSecret(content) ?? content);
}

/**
 * If the cursor sits inside a backtick span on the current line, returns the position right
 * after its closing backtick, so Tab can jump out of it. Covers secrets too (`` `!x!` ``).
 */
export function spanExit(value: string, cursor: number): number | null {
  const lineStart = value.lastIndexOf("\n", cursor - 1) + 1;
  const newline = value.indexOf("\n", cursor);
  const before = value.slice(lineStart, cursor);
  const after = value.slice(cursor, newline === -1 ? value.length : newline);
  const ticksBefore = before.split("`").length - 1;
  const tick = after.indexOf("`");
  return ticksBefore % 2 === 1 && tick !== -1 ? cursor + tick + 1 : null;
}
