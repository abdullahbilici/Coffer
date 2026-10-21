import { useLayoutEffect, useRef, type ReactNode } from "react";
import { motion } from "../../lib/motion";
import styles from "./Collapse.module.css";

/**
 * Closes its content to zero height while `leaving` is true, e.g. a tree branch being deleted.
 * Turning `leaving` off again (the delete failed or was cancelled) restores it.
 */
export function Collapse({ leaving, children }: { leaving: boolean; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !leaving) return;
    const { baseMs, easing } = motion();
    const animation = el.animate(
      [
        { height: `${el.offsetHeight}px`, opacity: 1 },
        { height: "0px", opacity: 0 },
      ],
      { duration: baseMs, easing, fill: "forwards" },
    );
    return () => animation.cancel();
  }, [leaving]);

  return (
    <div ref={ref} className={leaving ? styles.leaving : undefined}>
      {children}
    </div>
  );
}
