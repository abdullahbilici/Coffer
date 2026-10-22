const ICLOUD_DRIVE = /^\/Users\/[^/]+\/Library\/Mobile Documents\/com~apple~CloudDocs(?=\/|$)/;
const HOME = /^\/Users\/[^/]+(?=\/|$)/;

/** A path as Finder would name it: "iCloud Drive/Coffer", "~/Documents/Coffer". */
export function displayPath(path: string): string {
  return path.replace(ICLOUD_DRIVE, "iCloud Drive").replace(HOME, "~");
}

/** The last path component. */
export function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}
