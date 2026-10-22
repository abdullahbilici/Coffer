import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useStoredState } from "./useStoredState";

export type Appearance = "light" | "dark" | "system";

/**
 * Light, dark or the system's appearance. Applied to the native window, so WebKit's
 * `prefers-color-scheme` (and with it every token) follows without a CSS theme switch.
 */
export function useAppearance() {
  const [appearance, setAppearance] = useStoredState<Appearance>("appearance", "system");

  useEffect(() => {
    void getCurrentWindow().setTheme(appearance === "system" ? null : appearance);
  }, [appearance]);

  return [appearance, setAppearance] as const;
}
