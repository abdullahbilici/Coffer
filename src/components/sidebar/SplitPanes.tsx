import { useEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { cx } from "../../lib/cx";
import styles from "./SplitPanes.module.css";

export type Pane = {
  id: string;
  title: string;
  /** Header buttons; shown on hover, like VS Code view actions. */
  actions?: ReactNode;
  children: ReactNode;
};

type Layout = {
  collapsed: string[];
  /** Share of the space the top pane gets while both are open. */
  ratio: number;
};

const STORAGE_KEY = "coffer.sidebar.panes";
const DEFAULT_LAYOUT: Layout = { collapsed: [], ratio: 0.65 };
/** Smallest share either pane can be dragged to. */
const MIN_RATIO = 0.15;

// Layout is a per-viewer convenience; losing it just restores the default.
function loadLayout(): Layout {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? { ...DEFAULT_LAYOUT, ...(JSON.parse(raw) as Partial<Layout>) } : DEFAULT_LAYOUT;
  } catch {
    return DEFAULT_LAYOUT;
  }
}

function saveLayout(layout: Layout) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(layout));
  } catch {
    // Storage unavailable: keep the layout for this session only.
  }
}

/**
 * Two collapsible, independently scrolling panes with a draggable divider, like the views in
 * VS Code's sidebar.
 *
 * Panes are sized by `flex-grow` (0 = header only), which animates smoothly between states.
 * Content stays mounted while collapsed, so nothing re-renders or replays on open.
 */
export function SplitPanes({ top, bottom }: { top: Pane; bottom: Pane }) {
  const [layout, setLayout] = useState(loadLayout);
  const [resizing, setResizing] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => saveLayout(layout), [layout]);

  const topOpen = !layout.collapsed.includes(top.id);
  const bottomOpen = !layout.collapsed.includes(bottom.id);
  const both = topOpen && bottomOpen;
  const topGrow = !topOpen ? 0 : both ? layout.ratio : 1;
  const bottomGrow = !bottomOpen ? 0 : both ? 1 - layout.ratio : 1;

  const toggle = (id: string) =>
    setLayout((l) => ({
      ...l,
      collapsed: l.collapsed.includes(id)
        ? l.collapsed.filter((c) => c !== id)
        : [...l.collapsed, id],
    }));

  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    if (!container) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    setResizing(true);
    const rect = container.getBoundingClientRect();
    const move = (e: globalThis.PointerEvent) => {
      const ratio = (e.clientY - rect.top) / rect.height;
      setLayout((l) => ({ ...l, ratio: Math.min(1 - MIN_RATIO, Math.max(MIN_RATIO, ratio)) }));
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
      <PaneView pane={top} open={topOpen} grow={topGrow} onToggle={() => toggle(top.id)} />
      <PaneView
        pane={bottom}
        open={bottomOpen}
        grow={bottomGrow}
        onResize={both ? startResize : undefined}
        onToggle={() => toggle(bottom.id)}
      />
    </div>
  );
}

type PaneViewProps = {
  pane: Pane;
  open: boolean;
  /** Share of the free height; 0 leaves just the header. */
  grow: number;
  /** Present on the lower pane while both are open: drag its top edge to resize. */
  onResize?: (event: PointerEvent<HTMLDivElement>) => void;
  onToggle: () => void;
};

function PaneView({ pane, open, grow, onResize, onToggle }: PaneViewProps) {
  return (
    <section className={cx(styles.pane, open && styles.open)} style={{ flexGrow: grow }}>
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
