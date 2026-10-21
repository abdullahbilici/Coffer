import { useCallback, useEffect, useState } from "react";
import type { Run } from "../app/actions";
import type { MarkdownChips } from "../components/views/Markdown";
import { secretValues, toPlainText } from "../lib/markup";
import { toggled } from "../lib/sets";
import type { Session } from "./useSession";

type Options = {
  /** The readable text: a plain body, or the template with `` `!0!` `` markers. */
  readable: string;
  /** Anything to decrypt at all (inline secrets or a fully secure body). */
  protected: boolean;
  /** The whole body is one secret (a secure note), shown and copied as index 0. */
  wholeBody: boolean;
  /** Decrypts the full text; called once, on the first unhide or copy. */
  reveal: () => Promise<string>;
  session: Session;
  run: Run;
  /** Writes to the clipboard and confirms with a toast. */
  onCopy: (text: string) => Promise<void>;
};

/**
 * Masked secrets for one note body or overview. Decrypted text lives only in this hook's state:
 * it's fetched on the first unhide/copy (Touch ID only if the window is closed) and dropped as
 * soon as the unlock window closes or the owner unmounts.
 */
export function useSecrets(options: Options) {
  const { readable, wholeBody, reveal, session, run, onCopy } = options;
  const isProtected = options.protected;
  const [plaintext, setPlaintext] = useState<string | null>(null);
  const [shown, setShown] = useState<ReadonlySet<number>>(() => new Set());

  const forget = useCallback(() => {
    setPlaintext(null);
    setShown(new Set());
  }, []);

  useEffect(() => {
    if (!session.unlocked) forget();
  }, [session.unlocked, forget]);

  /** The full text; asks for Touch ID only when the unlock window is closed. */
  const full = async (): Promise<string> => {
    if (!isProtected) return readable;
    if (plaintext !== null) return plaintext;
    const text = await session.withUnlock(reveal);
    setPlaintext(text);
    return text;
  };

  const toggle = (index: number) =>
    run(async () => {
      if (!shown.has(index)) await full();
      setShown((prev) => toggled(prev, index));
    });

  const copySecret = (index: number) =>
    run(async () => {
      const text = await full();
      const value = wholeBody ? text : secretValues(text)[index];
      if (value !== undefined) await onCopy(value);
    });

  const copyAll = () => run(async () => onCopy(toPlainText(await full())));

  const chips: MarkdownChips = {
    secrets: wholeBody || plaintext === null ? null : secretValues(plaintext),
    shown,
    onToggleSecret: toggle,
    onCopySecret: copySecret,
    onCopyText: (text) => void run(() => onCopy(text)),
  };

  return { plaintext, shown, full, toggle, copyAll, forget, chips };
}
