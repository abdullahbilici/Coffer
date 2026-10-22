import { open } from "@tauri-apps/plugin-dialog";

/** Native folder picker; `null` when cancelled. */
export async function chooseFolder(defaultPath: string | null): Promise<string | null> {
  return open({ directory: true, defaultPath: defaultPath ?? undefined });
}

/** Native picker for a `.coffer` backup; `null` when cancelled. */
export async function chooseBackupFile(): Promise<string | null> {
  return open({ filters: [{ name: "Coffer backup", extensions: ["coffer"] }] });
}
