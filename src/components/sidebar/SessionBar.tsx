import { Lock, LockOpen, Settings } from "lucide-react";
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

type Props = { session: Session; onLock: () => void; onSettings: () => void };

export function SessionBar({ session, onLock, onSettings }: Props) {
  const settings = <IconButton icon={Settings} label="Settings" size="sm" onClick={onSettings} />;
  if (!session.unlocked) {
    return (
      <div className={styles.sessionBar}>
        <Lock size={14} strokeWidth={1.75} />
        <span className={styles.sessionState}>Secure items hidden</span>
        {settings}
      </div>
    );
  }
  return (
    <div className={styles.sessionBar} data-unlocked>
      <LockOpen size={14} strokeWidth={1.75} />
      <span>Unlocked</span>
      <span className={styles.countdown}>
        {session.expiresAt !== null && <Countdown until={session.expiresAt} />}
      </span>
      <IconButton icon={Lock} label="Lock now" size="sm" onClick={onLock} />
      {settings}
    </div>
  );
}

/** The only part of the app that ticks every second. */
function Countdown({ until }: { until: number }) {
  const now = useNow();
  return formatRemaining(until - now);
}
