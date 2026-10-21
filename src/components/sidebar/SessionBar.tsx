import { Lock, LockOpen } from "lucide-react";
import { useNow } from "../../hooks/useNow";
import type { Session } from "../../hooks/useSession";
import { IconButton } from "../ui/IconButton";
import styles from "./Sidebar.module.css";

function formatRemaining(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function SessionBar({ session, onLock }: { session: Session; onLock: () => void }) {
  if (!session.unlocked) {
    return (
      <div className={styles.sessionBar}>
        <Lock size={14} strokeWidth={1.75} />
        <span>Secure items hidden</span>
      </div>
    );
  }
  return (
    <div className={styles.sessionBar} data-unlocked>
      <LockOpen size={14} strokeWidth={1.75} />
      <span>Unlocked</span>
      {session.expiresAt !== null && <Countdown until={session.expiresAt} />}
      <IconButton icon={Lock} label="Lock now" size="sm" onClick={onLock} />
    </div>
  );
}

/** The only part of the app that ticks every second. */
function Countdown({ until }: { until: number }) {
  const now = useNow();
  return <span className={styles.countdown}>{formatRemaining(until - now)}</span>;
}
