import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, isLocked, type SessionInfo } from "../lib/api";
import { useNow } from "./useNow";

export type Session = ReturnType<typeof useSession>;

/** Mirrors `lock_events::SESSION_CHANGED`. */
const SESSION_CHANGED = "session-changed";

/**
 * Tracks the Touch ID unlock window. Revealed secrets are not cached here: each view fetches
 * them on demand and drops them when it unmounts or the window closes.
 */
export function useSession() {
  const [info, setInfo] = useState<SessionInfo>({ entered: false, expiresAt: null });
  const now = useNow();

  const unlocked = info.expiresAt !== null && info.expiresAt > now;
  const remainingMs = unlocked ? info.expiresAt! - now : 0;

  // The backend closes the window on its own when the screen locks or the Mac sleeps.
  useEffect(() => {
    const unlisten = listen<SessionInfo>(SESSION_CHANGED, (event) => setInfo(event.payload));
    return () => void unlisten.then((stop) => stop());
  }, []);

  const unlock = useCallback(async () => {
    setInfo(await api.unlock());
  }, []);

  const lock = useCallback(async () => {
    setInfo(await api.lock());
  }, []);

  /** Runs `action`; if the backend reports the window closed, asks for Touch ID and retries once. */
  const withUnlock = useCallback(
    async <T>(action: () => Promise<T>): Promise<T> => {
      try {
        return await action();
      } catch (error) {
        if (!isLocked(error)) throw error;
        await unlock();
        return action();
      }
    },
    [unlock],
  );

  /** Asks for Touch ID first if the window is closed. Use before entering any edit mode. */
  const ensureUnlocked = useCallback(async () => {
    if (info.expiresAt === null || info.expiresAt <= Date.now()) await unlock();
  }, [info.expiresAt, unlock]);

  return { entered: info.entered, unlocked, remainingMs, unlock, lock, withUnlock, ensureUnlocked };
}
