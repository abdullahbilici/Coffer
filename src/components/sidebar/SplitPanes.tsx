import { useRef, useState, type PointerEvent, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { useStoredState } from "../../hooks/useStoredState";
import { cx } from "../../lib/cx";
import styles from "./SplitPanes.module.css";

export type Pane = {
  id: string;
  title: string;
  /** Header buttons; shown on hover, like VS Code view actions. */
  actions?: ReactNode;
  /** Relative size when the layout has never been resized. */
  defaultWeight: number;
  children: ReactNode;
};

type Layout = {
  collapsed: string[];
  /** Relative size of each pane while open. */
  weights: Record<string, number>;
};

const STORAGE_KEY = "coffer.sidebar.panes.v2";
/** Smallest share of two neighbours either can be dragged to. */
const MIN_SHARE = 0.12;
const HEADER_PX = 26;

/**
 * Collapsible, independently scrolling panes with draggable dividers, like the views in
 * VS Code's sidebar.
 *
 * Panes are sized by `flex-grow` (0 = header only), which animates smoothly between states.
 * Content stays mounted while collapsed, so nothing re-renders or replays on open.
 */
export function SplitPanes({ panes }: { panes: Pane[] }) {
  const [layout, setLayout] = useStoredState<Layout>(STORAGE_KEY, { collapsed: [], weights: {} });
  const [resizing, setResizing] = useState(false);
  const refs = useRef(new Map<string, HTMLElement>());

  const isOpen = (pane: Pane) => !layout.collapsed.includes(pane.id);
  const weight = (pane: Pane) => layout.weights[pane.id] ?? pane.defaultWeight;

  const toggle = (id: string) =>
    setLayout((l) => ({
      ...l,
      collapsed: l.collapsed.includes(id)
        ? l.collapsed.filter((c) => c !== id)
        : [...l.collapsed, id],
    }));

  /** Drags the border between `above` and `below`, trading weight between just those two. */
  const startResize = (above: Pane, below: Pane) => (event: PointerEvent<HTMLDivElement>) => {
    const top = refs.current.get(above.id)?.getBoundingClientRect();
    const bottom = refs.current.get(below.id)?.getBoundingClientRect();
    if (!top || !bottom) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    setResizing(true);

    const total = weight(above) + weight(below);
    const space = bottom.bottom - top.top - 2 * HEADER_PX;
    const move = (e: globalThis.PointerEvent) => {
      const share = (e.clientY - top.top - HEADER_PX) / space;
      const clamped = Math.min(1 - MIN_SHARE, Math.max(MIN_SHARE, share));
      setLayout((l) => ({
        ...l,
        weights: { ...l.weights, [above.id]: total * clamped, [below.id]: total * (1 - clamped) },
      }));
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
    <div className={cx(styles.container, resizing && styles.resizing)}>
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
            grow={open ? weight(pane) : 0}
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
  /** Share of the free height; 0 leaves just the header. */
  grow: number;
  onResize?: (event: PointerEvent<HTMLDivElement>) => void;
  onToggle: () => void;
};

function PaneView({ ref, pane, open, grow, onResize, onToggle }: PaneViewProps) {
  return (
    <section ref={ref} className={cx(styles.pane, open && styles.open)} style={{ flexGrow: grow }}>
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
          <ChevronRight size={14} strokeWidth={2} className={styles.chevron} />
          {pane.title}
        </button>
        {pane.actions && <span className={styles.actions}>{pane.actions}</span>}
      </header>
      {/* `inert` keeps a collapsed pane's content out of focus and clicks. */}
      <div className={styles.body} inert={!open}>
        {pane.children}
      </div>
    </section>
  );
}
