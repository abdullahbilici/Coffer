import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Run } from "../app/actions";
import { api, type Settings } from "../lib/api";

/** Mirrors `backup::BACKUP_CHANGED`. */
const BACKUP_CHANGED = "backup-changed";

/** Settings stored in the vault, loaded while the Settings modal is open. */
export function useSettings(run: Run) {
  const [settings, setSettings] = useState<Settings | null>(null);

  const reload = useCallback(() => run(async () => setSettings(await api.getSettings())), [run]);

  useEffect(() => {
    void reload();
    // Automatic backups run in the background; keep the status line current.
    const unlisten = listen(BACKUP_CHANGED, () => void reload());
    return () => void unlisten.then((stop) => stop());
  }, [reload]);

  return { settings, setSettings, reload };
}
