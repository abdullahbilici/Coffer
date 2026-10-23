import { useEffect } from "react";
import { setTheme } from "@tauri-apps/api/app";
import { useStoredState } from "./useStoredState";

export type Appearance = "light" | "dark" | "system";

/**
 * Light, dark or the system's appearance. Applied to the whole app, so every window (quick
 * capture too) follows, and WebKit's `prefers-color-scheme` (with it every token) without a CSS
 * theme switch.
 */
export function useAppearance() {
  const [appearance, setAppearance] = useStoredState<Appearance>("appearance", "system");

  useEffect(() => {
    void setTheme(appearance === "system" ? null : appearance);
  }, [appearance]);

  return [appearance, setAppearance] as const;
}
