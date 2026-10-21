import type { KeyboardEvent } from "react";
import { COPYABLE, SECRET, spanExit, type Span } from "./markup";

/**
 * Replaces the textarea selection with `text`. Uses `insertText` so the change lands on the
 * native undo stack and fires the usual input event (React's onChange).
 */
export function insertText(el: HTMLTextAreaElement, text: string) {
  el.focus();
  if (!document.execCommand("insertText", false, text)) {
    el.setRangeText(text, el.selectionStart, el.selectionEnd, "end");
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }
}

/**
 * Wraps the selection in `span`, unwraps it if it is already wrapped, or inserts an empty span
 * with the cursor in the middle when nothing is selected.
 */
export function toggleSpan(el: HTMLTextAreaElement, { open, close }: Span) {
  const { value, selectionStart: start, selectionEnd: end } = el;
  const selected = value.slice(start, end);
  const wrapped =
    start !== end &&
    value.slice(start - open.length, start) === open &&
    value.slice(end, end + close.length) === close;

  if (wrapped) {
    el.setSelectionRange(start - open.length, end + close.length);
    insertText(el, selected);
    el.setSelectionRange(start - open.length, end - open.length);
  } else {
    insertText(el, open + selected + close);
    el.setSelectionRange(start + open.length, start + open.length + selected.length);
  }
}

/** Moves the cursor past the closing delimiter of the span it is in, followed by a space. */
export function exitSpan(el: HTMLTextAreaElement): boolean {
  if (el.selectionStart !== el.selectionEnd) return false;
  const target = spanExit(el.value, el.selectionStart);
  if (target === null) return false;
  el.setSelectionRange(target, target);
  if (el.value[target] === " ") el.setSelectionRange(target + 1, target + 1);
  else insertText(el, " ");
  return true;
}

/**
 * Editor shortcuts shared by every markup-aware textarea: Cmd+E copyable, Cmd+P secret,
 * Tab out of a span. Returns true when the key was handled.
 */
export function handleMarkupKeys(
  event: KeyboardEvent<HTMLTextAreaElement>,
  { secrets }: { secrets: boolean },
): boolean {
  const el = event.currentTarget;
  const key = event.key.toLowerCase();
  if (event.metaKey && key === "e") {
    toggleSpan(el, COPYABLE);
  } else if (event.metaKey && key === "p") {
    if (secrets) toggleSpan(el, SECRET);
  } else if (key !== "tab" || event.shiftKey || !exitSpan(el)) {
    return false;
  }
  event.preventDefault();
  return true;
}

/** Cmd+S saves and Esc cancels in an editor. Returns true when the key was handled. */
export function handleEditorKeys(
  event: KeyboardEvent<HTMLElement>,
  { save, cancel }: { save: () => void; cancel: () => void },
): boolean {
  if (event.metaKey && event.key.toLowerCase() === "s") save();
  else if (event.key === "Escape") cancel();
  else return false;
  event.preventDefault();
  return true;
}
