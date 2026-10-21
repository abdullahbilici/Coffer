import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { cx } from "../../lib/cx";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  pointerWithin,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type ClientRect,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { CornerDownLeft, type LucideIcon } from "lucide-react";
import type { Tree } from "../../lib/api";
import { Button } from "../ui/Button";
import styles from "./TreeDnd.module.css";

/** Where something is dropped: a project root or a folder. */
export type DropTarget = { projectId: number; folderId: number | null };

/** What is being dragged. */
export type DragSubject = {
  kind: "item" | "folder";
  id: number;
  label: string;
  icon: LucideIcon;
  /** Its current container, so dropping back there is a no-op. */
  from: DropTarget;
};

type DropData = {
  target: DropTarget;
  /** Name of the destination project or folder, for the confirmation. */
  label: string;
  /** `data-tree-node` of the destination row; the ghost sinks into it. */
  rowId: string;
  /** Hovering opens this collapsed node. */
  expandKey?: string;
};

/** Pointer travel before a press turns into a drag, so clicks still select. */
const DRAG_THRESHOLD_PX = 5;
/** Hovering a collapsed project or folder this long opens it. */
const EXPAND_DELAY_MS = 600;
/** Motion tokens from tokens.css, for the Web Animations API. Reduced motion makes them instant. */
function motion() {
  const css = getComputedStyle(document.documentElement);
  const ms = (name: string) => parseFloat(css.getPropertyValue(name)) || 0;
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  return {
    sinkMs: reduced ? 1 : ms("--duration-slow"),
    rowExitMs: reduced ? 1 : ms("--duration-base"),
    easing: css.getPropertyValue("--ease-out").trim() || "ease-out",
  };
}

type Rect = { left: number; top: number; width: number; height: number };

/** A drop waiting for confirmation. The ghost stays where it was released. */
type Pending = {
  subject: DragSubject;
  drop: DropData;
  ghost: Rect;
  origin: Rect;
};

type State = {
  active: DragSubject | null;
  target: DropTarget | null;
  /** Node id of the row animating out after a confirmed move. */
  leaving: string | null;
};

const DndState = createContext<State>({ active: null, target: null, leaving: null });

export const useTreeDnd = () => useContext(DndState);

export function sameTarget(a: DropTarget | null, b: DropTarget | null): boolean {
  return !!a && !!b && a.projectId === b.projectId && a.folderId === b.folderId;
}

const subjectNode = (s: DragSubject) => `${s.kind}:${s.id}`;

const toRect = (r: ClientRect | DOMRect): Rect => ({
  left: r.left,
  top: r.top,
  width: r.width,
  height: r.height,
});

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Folders a folder can't move into: itself and everything nested below it. */
function blockedFolders(tree: Tree, folderId: number): Set<number> {
  const blocked = new Set([folderId]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const f of tree.folders) {
      if (f.parentId !== null && blocked.has(f.parentId) && !blocked.has(f.id)) {
        blocked.add(f.id);
        grew = true;
      }
    }
  }
  return blocked;
}

type ProviderProps = {
  tree: Tree;
  expanded: ReadonlySet<string>;
  onExpand: (keys: string[]) => void;
  /** Asks for Touch ID if the unlock window is closed; rejects if the user cancels. */
  authorize: () => Promise<void>;
  onMove: (subject: DragSubject, target: DropTarget) => void;
  children: ReactNode;
};

export function TreeDndProvider(props: ProviderProps) {
  const { tree, expanded, onExpand, authorize, onMove, children } = props;
  const [active, setActive] = useState<DragSubject | null>(null);
  const [blocked, setBlocked] = useState<ReadonlySet<number>>(() => new Set());
  const [over, setOver] = useState<DropData | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [leaving, setLeaving] = useState<string | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: DRAG_THRESHOLD_PX } }),
  );

  const valid = (target: DropTarget | null): target is DropTarget =>
    !!active &&
    !!target &&
    !sameTarget(target, active.from) &&
    !(target.folderId !== null && blocked.has(target.folderId));

  // While a drop waits for confirmation its destination stays highlighted.
  const target = pending ? pending.drop.target : over && valid(over.target) ? over.target : null;

  // Open a collapsed container after hovering it for a moment.
  const expandKey = over?.expandKey;
  useEffect(() => {
    if (!active || !expandKey || expanded.has(expandKey)) return;
    const timer = setTimeout(() => onExpand([expandKey]), EXPAND_DELAY_MS);
    return () => clearTimeout(timer);
  }, [active, expandKey, expanded, onExpand]);

  // Grabbing cursor everywhere while dragging.
  useEffect(() => {
    document.documentElement.classList.toggle(styles.dragging, !!active);
  }, [active]);

  const onDragStart = ({ active }: DragStartEvent) => {
    const subject = active.data.current as DragSubject;
    setActive(subject);
    setBlocked(subject.kind === "folder" ? blockedFolders(tree, subject.id) : new Set());
  };

  const onDragOver = ({ over }: DragOverEvent) => setOver((over?.data.current as DropData) ?? null);

  const onDragEnd = ({ active: dragged, over }: DragEndEvent) => {
    const drop = over?.data.current as DropData | undefined;
    const ghost = dragged.rect.current.translated;
    const origin = dragged.rect.current.initial;
    if (active && drop && valid(drop.target) && ghost && origin) {
      setPending({ subject: active, drop, ghost: toRect(ghost), origin: toRect(origin) });
    }
    setActive(null);
    setOver(null);
  };

  const onDragCancel = () => {
    setActive(null);
    setOver(null);
  };

  const confirm = async () => {
    if (!pending) return;
    try {
      await authorize();
    } catch {
      return "cancel" as const;
    }
    const node = subjectNode(pending.subject);
    setLeaving(node);
    // Let the source row close before the tree reloads with the row in its new place.
    void wait(motion().rowExitMs).then(() => {
      onMove(pending.subject, pending.drop.target);
      setLeaving((current) => (current === node ? null : current));
    });
    return "sink" as const;
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={pointerWithin}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragEnd={onDragEnd}
      onDragCancel={onDragCancel}
    >
      <DndState.Provider value={{ active, target, leaving }}>{children}</DndState.Provider>
      {/* Valid drops hand the ghost over to `PendingMove`; others fly back to where they came from. */}
      <DragOverlay dropAnimation={pending ? null : undefined}>
        {active && <DragGhost subject={active} />}
      </DragOverlay>
      {pending && (
        <PendingMove
          key={`${subjectNode(pending.subject)}:${pending.ghost.left}:${pending.ghost.top}`}
          pending={pending}
          onConfirm={confirm}
          onDone={() => setPending(null)}
        />
      )}
    </DndContext>
  );
}

function DragGhost({ subject }: { subject: DragSubject }) {
  const Icon = subject.icon;
  return (
    <div className={styles.ghost}>
      <Icon size={16} strokeWidth={1.75} className={styles.ghostIcon} />
      <span className={styles.ghostLabel}>{subject.label}</span>
    </div>
  );
}

type PendingProps = {
  pending: Pending;
  onConfirm: () => Promise<"sink" | "cancel" | undefined>;
  onDone: () => void;
};

/**
 * The released ghost plus a confirmation under the destination row. Confirming sinks the ghost
 * into the destination; cancelling flies it back to its origin.
 */
function PendingMove({ pending, onConfirm, onDone }: PendingProps) {
  const ghostRef = useRef<HTMLDivElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const [closing, setClosing] = useState(false);
  // Enter and the focused Move button can both fire; only the first one counts.
  const settled = useRef(false);
  const [position, setPosition] = useState({ left: 0, top: 0 });

  const row = document.querySelector(`[data-tree-node="${pending.drop.rowId}"]`);
  const destination = row ? toRect(row.getBoundingClientRect()) : pending.ghost;

  // Below the destination row, kept inside the window.
  useLayoutEffect(() => {
    const box = popoverRef.current?.getBoundingClientRect();
    if (!box) return;
    const margin = 8;
    setPosition({
      left: Math.min(destination.left + 8, window.innerWidth - box.width - margin),
      top: Math.min(
        destination.top + destination.height + 6,
        window.innerHeight - box.height - margin,
      ),
    });
    // Measured once, when the confirmation opens.
  }, []);

  const finish = (to: Rect, sink: boolean) => {
    setClosing(true);
    const ghost = ghostRef.current;
    if (!ghost) return onDone();
    const dx = to.left + to.height / 2 - pending.ghost.left - pending.ghost.height / 2;
    const dy = to.top - pending.ghost.top;
    const end = sink
      ? `translate(${dx}px, ${dy}px) scale(0.2)`
      : `translate(${to.left - pending.ghost.left}px, ${dy}px)`;
    ghost
      .animate(
        [
          { transform: "rotate(-1.5deg) scale(1.03)", opacity: 1 },
          { transform: end, opacity: 0 },
        ],
        { duration: motion().sinkMs, easing: motion().easing, fill: "forwards" },
      )
      .finished.then(onDone, onDone);
  };

  const confirm = async () => {
    if (settled.current) return;
    settled.current = true;
    const result = await onConfirm();
    finish(result === "sink" ? destination : pending.origin, result === "sink");
  };

  const cancel = () => {
    if (settled.current) return;
    settled.current = true;
    finish(pending.origin, false);
  };

  // Enter confirms, Esc cancels. Capture phase so nothing else reacts to these keys.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Enter" && event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Enter") void confirm();
      else cancel();
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  });

  const Icon = pending.subject.icon;
  return createPortal(
    <>
      <div
        ref={ghostRef}
        className={cx(styles.ghost, styles.released)}
        style={{ left: pending.ghost.left, top: pending.ghost.top }}
      >
        <Icon size={16} strokeWidth={1.75} className={styles.ghostIcon} />
        <span className={styles.ghostLabel}>{pending.subject.label}</span>
      </div>
      <div
        ref={popoverRef}
        role="dialog"
        aria-label="Confirm move"
        className={cx(styles.confirm, closing && styles.confirmClosing)}
        style={{ left: position.left, top: position.top }}
      >
        <p className={styles.confirmText}>
          Move <strong>{pending.subject.label}</strong> to <strong>{pending.drop.label}</strong>
        </p>
        <div className={styles.confirmActions}>
          <Button variant="ghost" size="sm" onClick={cancel}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" autoFocus onClick={() => void confirm()}>
            Move
            <CornerDownLeft size={13} strokeWidth={2} />
          </Button>
        </div>
      </div>
    </>,
    document.body,
  );
}

type NodeOptions = {
  /** Unique per row, e.g. `folder:3`; also set as `data-tree-node`. */
  id: string;
  /** Omit for rows that can't be dragged (projects). */
  subject?: DragSubject;
  drop: DropData;
};

/** Wires a tree row up as a draggable (optional) and a drop target. */
export function useTreeNode({ id, subject, drop }: NodeOptions) {
  const draggable = useDraggable({ id: `drag:${id}`, data: subject, disabled: !subject });
  const droppable = useDroppable({ id: `drop:${id}`, data: drop });
  const { leaving } = useTreeDnd();
  return {
    id,
    ref: (el: HTMLElement | null) => {
      draggable.setNodeRef(el);
      droppable.setNodeRef(el);
    },
    // Only the listeners: dnd-kit's attributes would override the row's tree semantics.
    handleProps: subject ? draggable.listeners : undefined,
    isDragging: draggable.isDragging,
    leaving: leaving === id,
  };
}
