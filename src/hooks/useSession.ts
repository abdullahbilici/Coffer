import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, isLocked, type SessionInfo } from "../lib/api";

export type Session = ReturnType<typeof useSession>;

/** Mirrors `lock_events::SESSION_CHANGED`. */
const SESSION_CHANGED = "session-changed";

/**
 * Tracks the Touch ID unlock window. Revealed secrets are not cached here: each view fetches
 * them on demand and drops them when it unmounts or the window closes.
 *
 * Nothing here ticks: `unlocked` flips with one timer at the window's end, and the returned
 * object only changes when the session does, so it doesn't re-render the app every second.
 * The live countdown lives in `SessionBar`.
 */
export function useSession() {
  const [info, setInfo] = useState<SessionInfo>({ entered: false, expiresAt: null });
  const [unlocked, setUnlocked] = useState(false);

  useEffect(() => {
    const remaining = info.expiresAt === null ? 0 : info.expiresAt - Date.now();
    setUnlocked(remaining > 0);
    if (remaining <= 0) return;
    const timer = setTimeout(() => setUnlocked(false), remaining);
    return () => clearTimeout(timer);
  }, [info.expiresAt]);

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

  return useMemo(
    () => ({
      entered: info.entered,
      unlocked,
      expiresAt: info.expiresAt,
      unlock,
      lock,
      withUnlock,
      ensureUnlocked,
    }),
    [info.entered, info.expiresAt, unlocked, unlock, lock, withUnlock, ensureUnlocked],
  );
}
