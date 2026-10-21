import { useEffect, useState } from "react";

/**
 * `useState` that survives restarts via `localStorage`. Only for per-viewer UI preferences
 * (view modes, pane sizes); if storage is unavailable it silently falls back to `initial`.
 */
export function useStoredState<T>(key: string, initial: T) {
  const [value, setValue] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(key);
      return raw === null ? initial : (JSON.parse(raw) as T);
    } catch {
      return initial;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      // Storage unavailable: keep the value for this session only.
    }
  }, [key, value]);

  return [value, setValue] as const;
}
