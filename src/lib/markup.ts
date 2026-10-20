/**
 * Inline markup in item bodies:
 * - `{{value}}` is a secret. The backend stores it encrypted and leaves a `{{0}}`-style marker
 *   in the readable template (see `src-tauri/src/secrets.rs`).
 * - `` `value` `` is a copyable value. Not secret; shown as a click-to-copy chip.
 */

export type Segment =
  | { kind: "text"; text: string }
  | { kind: "copy"; text: string }
  /** `text` is `null` while the secret has not been fetched. */
  | { kind: "secret"; index: number; text: string | null };

export type Span = { open: string; close: string };

export const SECRET: Span = { open: "{{", close: "}}" };
export const COPYABLE: Span = { open: "`", close: "`" };

const TEMPLATE_MARKER = /\{\{(\d+)\}\}/g;
const SECRET_SPAN = /\{\{([\s\S]+?)\}\}/g;
const COPY_SPAN = /`([^`\n]+)`/g;

function splitCopyable(text: string): Segment[] {
  const segments: Segment[] = [];
  let last = 0;
  for (const match of text.matchAll(COPY_SPAN)) {
    if (match.index > last) segments.push({ kind: "text", text: text.slice(last, match.index) });
    segments.push({ kind: "copy", text: match[1] });
    last = match.index + match[0].length;
  }
  if (last < text.length) segments.push({ kind: "text", text: text.slice(last) });
  return segments;
}

// Secrets are split first, like the backend does, so a backtick never swallows a secret.
function parse(body: string, pattern: RegExp, value: (match: string) => string | null): Segment[] {
  const segments: Segment[] = [];
  let last = 0;
  let index = 0;
  for (const match of body.matchAll(pattern)) {
    segments.push(...splitCopyable(body.slice(last, match.index)));
    segments.push({ kind: "secret", index: index++, text: value(match[1]) });
    last = match.index + match[0].length;
  }
  segments.push(...splitCopyable(body.slice(last)));
  return segments;
}

/** Segments of a stored template; secrets are not known yet. */
export function parseTemplate(template: string): Segment[] {
  return parse(template, TEMPLATE_MARKER, () => null);
}

/** Segments of a full body with secret values filled in. */
export function parseBody(body: string): Segment[] {
  return parse(body, SECRET_SPAN, (value) => value);
}

/** Segments of a fully encrypted body: only copyable chips, no inline secrets. */
export function parseCopyable(body: string): Segment[] {
  return splitCopyable(body);
}

export function secretValues(body: string): string[] {
  return [...body.matchAll(SECRET_SPAN)].map((match) => match[1]);
}

/** The body without markup, for "Copy all". */
export function toPlainText(body: string): string {
  return body.replace(SECRET_SPAN, "$1").replace(COPY_SPAN, "$1");
}

/**
 * If the cursor sits inside a span on the current line, returns the position right after the
 * span's closing delimiter, so Tab can jump out of it.
 */
export function spanExit(value: string, cursor: number): number | null {
  const lineStart = value.lastIndexOf("\n", cursor - 1) + 1;
  const newline = value.indexOf("\n", cursor);
  const before = value.slice(lineStart, cursor);
  const after = value.slice(cursor, newline === -1 ? value.length : newline);

  const close = after.indexOf(SECRET.close);
  if (before.lastIndexOf(SECRET.open) > before.lastIndexOf(SECRET.close) && close !== -1) {
    return cursor + close + SECRET.close.length;
  }
  const tick = after.indexOf(COPYABLE.close);
  const ticksBefore = before.split(COPYABLE.open).length - 1;
  if (ticksBefore % 2 === 1 && tick !== -1) return cursor + tick + COPYABLE.close.length;
  return null;
}
