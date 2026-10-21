import { Pin } from "lucide-react";

/** The small pin mark next to pinned rows, tiles and list entries. */
export function PinBadge({ className }: { className?: string }) {
  return <Pin size={12} strokeWidth={1.75} className={className} aria-label="Pinned" />;
}
