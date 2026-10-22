import { useState } from "react";
import { Trash2 } from "lucide-react";
import { Button } from "./Button";
import { CONFIRM_DELETE } from "./Menu";

/** Ghost "Delete" that turns into "Confirm delete" and only deletes on the second click. */
export function DeleteButton({ onDelete }: { onDelete: () => void }) {
  const [confirming, setConfirming] = useState(false);
  return (
    <Button
      variant={confirming ? "danger" : "ghost"}
      size="sm"
      icon={Trash2}
      onClick={() => (confirming ? onDelete() : setConfirming(true))}
      onMouseLeave={() => setConfirming(false)}
    >
      {confirming ? CONFIRM_DELETE : "Delete"}
    </Button>
  );
}
