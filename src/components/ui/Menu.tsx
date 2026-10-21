import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { LucideIcon } from "lucide-react";
import { cx } from "../../lib/cx";
import styles from "./Menu.module.css";

/** Second-click label for every destructive menu entry. */
export const CONFIRM_DELETE = "Confirm delete";

export type MenuItem = {
  label: string;
  icon: LucideIcon;
  onSelect: () => void;
  danger?: boolean;
  /** When set, the first click shows this label and only the second click runs `onSelect`. */
  confirmLabel?: string;
};

export type MenuAnchor = { x: number; y: number };

type Props = {
  anchor: MenuAnchor;
  items: MenuItem[];
  onClose: () => void;
};

const VIEWPORT_MARGIN = 8;

export function Menu({ anchor, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [armed, setArmed] = useState<number | null>(null);
  const [position, setPosition] = useState(anchor);

  // Keep the menu inside the window.
  useLayoutEffect(() => {
    const rect = ref.current?.getBoundingClientRect();
    if (!rect) return;
    setPosition({
      x: Math.min(anchor.x, window.innerWidth - rect.width - VIEWPORT_MARGIN),
      y: Math.min(anchor.y, window.innerHeight - rect.height - VIEWPORT_MARGIN),
    });
  }, [anchor]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onClose();
    };
    window.addEventListener("pointerdown", onPointerDown);
    // Capture phase: runs before the app-wide Esc handler.
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  const select = (item: MenuItem, index: number) => {
    if (item.confirmLabel && armed !== index) {
      setArmed(index);
      return;
    }
    onClose();
    item.onSelect();
  };

  return createPortal(
    <div
      ref={ref}
      role="menu"
      className={styles.menu}
      style={{ left: position.x, top: position.y }}
    >
      {items.map((item, index) => {
        const Icon = item.icon;
        const isArmed = armed === index;
        return (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            className={cx(styles.item, item.danger && styles.danger, isArmed && styles.armed)}
            onClick={() => select(item, index)}
          >
            <Icon size={14} strokeWidth={1.75} />
            {isArmed ? item.confirmLabel : item.label}
          </button>
        );
      })}
    </div>,
    document.body,
  );
}
