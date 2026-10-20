/**
 * Inline markup in item bodies and project overviews. Mirrors `src-tauri/src/secrets.rs`.
 * - `` `value` `` is a copyable value; not secret.
 * - `` `!value!` `` is a secret. The backtick and `!` must touch on both sides; `` ` !x! ` `` is
 *   just a copyable value. The backend stores secrets encrypted and leaves `` `!0!` ``-style
 *   markers in the readable template.
 */

export type Segment =
  | { kind: "text"; text: string }
  | { kind: "copy"; text: string }
  /** `text` is `null` while the secret has not been fetched. */
  | { kind: "secret"; index: number; text: string | null };

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

function parse(body: string, secretAt: (content: string, order: number) => Segment | null) {
  const segments: Segment[] = [];
  let last = 0;
  let order = 0;
  for (const match of body.matchAll(SPAN)) {
    if (match.index > last) segments.push({ kind: "text", text: body.slice(last, match.index) });
    const secret = secretAt(match[1], order);
    if (secret) order++;
    segments.push(secret ?? { kind: "copy", text: match[1] });
    last = match.index + match[0].length;
  }
  if (last < body.length) segments.push({ kind: "text", text: body.slice(last) });
  return segments;
}

/** Segments of a stored template; secrets are not known yet. */
export function parseTemplate(template: string): Segment[] {
  return parse(template, (content) => {
    const index = asMarker(content);
    return index === null ? null : { kind: "secret", index, text: null };
  });
}

/** Segments of a full body with secret values filled in. */
export function parseBody(body: string): Segment[] {
  return parse(body, (content, order) => {
    const text = asSecret(content);
    return text === null ? null : { kind: "secret", index: order, text };
  });
}

/** Segments of a fully encrypted body: only copyable chips, no inline secrets. */
export function parseCopyable(body: string): Segment[] {
  return parse(body, () => null);
}

export function secretValues(body: string): string[] {
  return parseBody(body).flatMap((s) => (s.kind === "secret" && s.text !== null ? [s.text] : []));
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
