import { useCallback, useEffect, useState } from "react";
import { api, type Tree } from "../lib/api";

/** Loads the sidebar tree once the vault is entered. Call `reload` after every mutation. */
export function useVault(entered: boolean) {
  const [tree, setTree] = useState<Tree | null>(null);

  const reload = useCallback(async () => {
    setTree(await api.getTree());
  }, []);

  useEffect(() => {
    if (entered) void reload();
  }, [entered, reload]);

  return { tree, reload };
}
