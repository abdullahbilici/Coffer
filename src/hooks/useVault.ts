import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Tree } from "../lib/api";

/** Mirrors `commands::assistant::VAULT_CHANGED`. */
const VAULT_CHANGED = "vault-changed";

/** Loads the sidebar tree once the vault is entered. Call `reload` after every mutation. */
export function useVault(entered: boolean) {
  const [tree, setTree] = useState<Tree | null>(null);

  const reload = useCallback(async () => {
    setTree(await api.getTree());
  }, []);

  useEffect(() => {
    if (entered) void reload();
  }, [entered, reload]);

  // A draft applied in quick capture changed the vault behind this window's back.
  useEffect(() => {
    if (!entered) return;
    const unlisten = listen(VAULT_CHANGED, () => void reload().catch(() => {}));
    return () => void unlisten.then((stop) => stop());
  }, [entered, reload]);

  return { tree, reload };
}
