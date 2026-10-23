import { useCallback, useEffect, useState } from "react";
import type { Copy, Run } from "../app/actions";
import type { MarkdownChips } from "../components/views/Markdown";
import { api } from "../lib/api";
import { toggled } from "../lib/sets";
import type { Session } from "./useSession";

/**
 * Secret chips for an assistant reply. `handles[k]` names the secret behind marker k (see
 * `withSecretChips`); each value is fetched on its first unhide or copy (Touch ID only if the
 * window is closed) and dropped when the window closes, like `useSecrets`.
 */
export function useHandleChips(
  handles: string[],
  session: Session,
  run: Run,
  onCopy: Copy,
): MarkdownChips {
  const [values, setValues] = useState<ReadonlyMap<number, string>>(() => new Map());
  const [shown, setShown] = useState<ReadonlySet<number>>(() => new Set());

  const forget = useCallback(() => {
    setValues(new Map());
    setShown(new Set());
  }, []);

  useEffect(() => {
    if (!session.unlocked) forget();
  }, [session.unlocked, forget]);

  const value = async (index: number): Promise<string> => {
    const known = values.get(index);
    if (known !== undefined) return known;
    const fetched = await session.withUnlock(() => api.revealHandle(handles[index]));
    setValues((prev) => new Map(prev).set(index, fetched));
    return fetched;
  };

  const secrets: string[] = [];
  values.forEach((v, i) => (secrets[i] = v));

  return {
    secrets,
    shown,
    onToggleSecret: (index) =>
      void run(async () => {
        if (!shown.has(index)) await value(index);
        setShown((prev) => toggled(prev, index));
      }),
    onCopySecret: (index) => void run(async () => onCopy(await value(index), true)),
    onCopyText: (text) => void run(() => onCopy(text)),
  };
}
