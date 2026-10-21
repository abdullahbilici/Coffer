/**
 * Motion tokens from tokens.css for code that animates outside CSS (timers, Web Animations).
 * Reduced motion makes every duration effectively instant.
 */
export function motion() {
  const css = getComputedStyle(document.documentElement);
  const ms = (name: string) => parseFloat(css.getPropertyValue(name)) || 0;
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  return {
    baseMs: reduced ? 1 : ms("--duration-base"),
    slowMs: reduced ? 1 : ms("--duration-slow"),
    glideMs: reduced ? 1 : ms("--duration-glide"),
    easing: css.getPropertyValue("--ease-out").trim() || "ease-out",
    lifted: css.getPropertyValue("--transform-lifted").trim() || "none",
  };
}
