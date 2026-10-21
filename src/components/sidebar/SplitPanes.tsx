import { useEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { useStoredState } from "../../hooks/useStoredState";
import { cx } from "../../lib/cx";
import styles from "./SplitPanes.module.css";

export type Pane = {
  id: string;
  title: string;
  /** Header buttons; shown on hover, like VS Code view actions. */
  actions?: ReactNode;
  /**
   * The main pane takes whatever height is left. Other panes are only as tall as their
   * content, up to a cap, so a short or empty list never claims space it can't use.
   */
  fill?: boolean;
  children: ReactNode;
};

type Layout = {
  collapsed: string[];
  /** Height caps (px) the user dragged for content-sized panes. */
  caps: Record<string, number>;
};

const STORAGE_KEY = "coffer.sidebar.panes.v3";
/** Default cap for a content-sized pane, as a share of the sidebar's height. */
const DEFAULT_CAP = 0.35;
/** Smallest height a pane can be dragged to: its header plus a couple of rows. */
const MIN_PANE_PX = 84;

/**
 * Collapsible, independently scrolling panes like VS Code's sidebar views. Content-sized panes
 * open and close by animating their body height; the fill pane animates its flex-grow. Content
 * stays mounted while collapsed, so nothing re-renders or replays on open.
 */
export function SplitPanes({ panes }: { panes: Pane[] }) {
  const [layout, setLayout] = useStoredState<Layout>(STORAGE_KEY, { collapsed: [], caps: {} });
  const [resizing, setResizing] = useState(false);
  const [height, setHeight] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const refs = useRef(new Map<string, HTMLElement>());

  // Caps scale with the sidebar, so track its height.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setHeight(entry.contentRect.height));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const isOpen = (pane: Pane) => !layout.collapsed.includes(pane.id);
  const capOf = (pane: Pane) =>
    Math.min(layout.caps[pane.id] ?? height * DEFAULT_CAP, Math.max(height - MIN_PANE_PX, 0));

  const toggle = (id: string) =>
    setLayout((l) => ({
      ...l,
      collapsed: l.collapsed.includes(id)
        ? l.collapsed.filter((c) => c !== id)
        : [...l.collapsed, id],
    }));

  /**
   * Drags the border between two open panes. The content-sized one of the pair gets a new cap;
   * the fill pane simply takes the rest.
   */
  const startResize = (above: Pane, below: Pane) => (event: PointerEvent<HTMLDivElement>) => {
    const sized = above.fill ? below : above;
    const el = refs.current.get(sized.id);
    if (!el) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    setResizing(true);
    const rect = el.getBoundingClientRect();
    const move = (e: globalThis.PointerEvent) => {
      const next = sized === above ? e.clientY - rect.top : rect.bottom - e.clientY;
      const clamped = Math.max(MIN_PANE_PX, Math.min(next, height - MIN_PANE_PX));
      setLayout((l) => ({ ...l, caps: { ...l.caps, [sized.id]: clamped } }));
    };
    const end = () => {
      setResizing(false);
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      handle.removeEventListener("pointercancel", end);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  };

  return (
    <div ref={containerRef} className={cx(styles.container, resizing && styles.resizing)}>
      {panes.map((pane, i) => {
        const open = isOpen(pane);
        // A divider sits on this pane's top edge when it and the nearest open pane above are open.
        const above = open ? panes.slice(0, i).reverse().find(isOpen) : undefined;
        return (
          <PaneView
            key={pane.id}
            ref={(el) => {
              if (el) refs.current.set(pane.id, el);
              else refs.current.delete(pane.id);
            }}
            pane={pane}
            open={open}
            cap={pane.fill ? undefined : capOf(pane)}
            onResize={above ? startResize(above, pane) : undefined}
            onToggle={() => toggle(pane.id)}
          />
        );
      })}
    </div>
  );
}

type PaneViewProps = {
  ref: (el: HTMLElement | null) => void;
  pane: Pane;
  open: boolean;
  /** Max height of a content-sized pane; `undefined` for the fill pane. */
  cap?: number;
  onResize?: (event: PointerEvent<HTMLDivElement>) => void;
  onToggle: () => void;
};

function PaneView({ ref, pane, open, cap, onResize, onToggle }: PaneViewProps) {
  return (
    <section
      ref={ref}
      className={cx(styles.pane, pane.fill ? styles.fill : styles.sized, open && styles.open)}
      style={cap === undefined ? undefined : { maxHeight: cap }}
    >
      {onResize && (
        <div
          className={styles.divider}
          role="separator"
          aria-orientation="horizontal"
          onPointerDown={onResize}
        />
      )}
      <header className={styles.header}>
        <button type="button" className={styles.toggle} aria-expanded={open} onClick={onToggle}>
          <ChevronRight size={14} strokeWidth={1.75} className={styles.chevron} />
          {pane.title}
        </button>
        {pane.actions && <span className={styles.actions}>{pane.actions}</span>}
      </header>
      {/* Grid rows animate between 0fr and 1fr, so the body opens to exactly its content.
          `inert` keeps a collapsed pane's content out of focus and clicks. */}
      <div className={styles.bodyWrap} inert={!open}>
        <div className={styles.body}>
          <div className={styles.content}>{pane.children}</div>
        </div>
      </div>
    </section>
  );
}
