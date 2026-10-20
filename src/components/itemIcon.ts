import { FileKey, FileText, KeyRound, type LucideIcon } from "lucide-react";
import type { ItemSummary } from "../lib/api";

/** Secure items, items with inline secrets and plain items each get their own icon. */
export function itemIcon(item: ItemSummary): LucideIcon {
  if (item.secure) return KeyRound;
  if (item.hasSecrets) return FileKey;
  return FileText;
}
