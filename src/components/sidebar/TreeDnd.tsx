import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  pointerWithin,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import type { LucideIcon } from "lucide-react";
import type { Tree } from "../../lib/api";
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

type DropData = { target: DropTarget; expandKey?: string };

/** Pointer travel before a press turns into a drag, so clicks still select. */
const DRAG_THRESHOLD_PX = 5;
/** Hovering a collapsed project or folder this long opens it. */
const EXPAND_DELAY_MS = 600;

type State = { active: DragSubject | null; target: DropTarget | null };

const DndState = createContext<State>({ active: null, target: null });

export const useTreeDnd = () => useContext(DndState);

export function sameTarget(a: DropTarget | null, b: DropTarget | null): boolean {
  return !!a && !!b && a.projectId === b.projectId && a.folderId === b.folderId;
}

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
  onMove: (subject: DragSubject, target: DropTarget) => void;
  children: ReactNode;
};

export function TreeDndProvider({ tree, expanded, onExpand, onMove, children }: ProviderProps) {
  const [active, setActive] = useState<DragSubject | null>(null);
  const [blocked, setBlocked] = useState<ReadonlySet<number>>(() => new Set());
  const [over, setOver] = useState<DropData | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: DRAG_THRESHOLD_PX } }),
  );

  const valid = (target: DropTarget | null): target is DropTarget =>
    !!active &&
    !!target &&
    !sameTarget(target, active.from) &&
    !(target.folderId !== null && blocked.has(target.folderId));

  const target = over && valid(over.target) ? over.target : null;

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

  const reset = () => {
    setActive(null);
    setOver(null);
  };

  const onDragStart = ({ active }: DragStartEvent) => {
    const subject = active.data.current as DragSubject;
    setActive(subject);
    setBlocked(subject.kind === "folder" ? blockedFolders(tree, subject.id) : new Set());
  };

  const onDragOver = ({ over }: DragOverEvent) => setOver((over?.data.current as DropData) ?? null);

  const onDragEnd = ({ over }: DragEndEvent) => {
    const drop = over?.data.current as DropData | undefined;
    if (active && drop && valid(drop.target)) onMove(active, drop.target);
    reset();
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={pointerWithin}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragEnd={onDragEnd}
      onDragCancel={reset}
    >
      <DndState.Provider value={{ active, target }}>{children}</DndState.Provider>
      {/* No drop animation on a valid drop: the row re-mounts in its new place instead. */}
      <DragOverlay dropAnimation={target ? null : undefined}>
        {active && <DragGhost subject={active} />}
      </DragOverlay>
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

type NodeOptions = {
  /** Unique per row, e.g. `folder:3`. */
  id: string;
  /** Omit for rows that can't be dragged (projects). */
  subject?: DragSubject;
  drop: DropData;
};

/** Wires a tree row up as a draggable (optional) and a drop target. */
export function useTreeNode({ id, subject, drop }: NodeOptions) {
  const draggable = useDraggable({ id: `drag:${id}`, data: subject, disabled: !subject });
  const droppable = useDroppable({ id: `drop:${id}`, data: drop });
  return {
    ref: (el: HTMLElement | null) => {
      draggable.setNodeRef(el);
      droppable.setNodeRef(el);
    },
    // Only the listeners: dnd-kit's attributes would override the row's tree semantics.
    handleProps: subject ? draggable.listeners : undefined,
    isDragging: draggable.isDragging,
  };
}
