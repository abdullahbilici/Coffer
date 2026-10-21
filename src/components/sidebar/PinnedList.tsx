import { useEffect, useRef, useState, type MouseEvent } from "react";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  closestCenter,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  defaultDropAnimationSideEffects,
  type DragEndEvent,
  type DropAnimation,
} from "@dnd-kit/core";
import { Box, Folder, PinOff, type LucideIcon } from "lucide-react";
import type { VaultActions } from "../../app/actions";
import type { PinRef, Tree } from "../../lib/api";
import { cx } from "../../lib/cx";
import { motion } from "../../lib/motion";
import { pinKey } from "../../lib/pins";
import type { Selection } from "../../lib/selection";
import { itemIcon } from "../itemIcon";
import { IconButton } from "../ui/IconButton";
import { Menu, type MenuAnchor } from "../ui/Menu";
import { TreeRow } from "./TreeRow";
import styles from "./Sidebar.module.css";

/** Pointer travel before a press turns into a drag, so clicks still open the pin. */
const DRAG_THRESHOLD_PX = 5;

type Resolved = {
  pin: PinRef;
  label: string;
  icon: LucideIcon;
  /** Where it lives, shown dimmed on the right. */
  location: string;
  open: () => void;
  selected: boolean;
};

type Props = {
  tree: Tree;
  selection: Selection;
  actions: VaultActions;
};

/** Pinned projects, folders and notes in the user's order; drag to reorder. */
export function PinnedList({ tree, selection, actions }: Props) {
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ anchor: MenuAnchor; pin: PinRef } | null>(null);
  // A dropped order shows immediately, so the drop animation lands on the new slot instead of
  // flying back while the backend catches up.
  const [optimistic, setOptimistic] = useState<string[] | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: DRAG_THRESHOLD_PX } }),
  );

  const projectName = (id: number) => tree.projects.find((p) => p.id === id)?.name ?? "";
  const ordered = optimistic
    ? [...tree.pins].sort((a, b) => optimistic.indexOf(pinKey(a)) - optimistic.indexOf(pinKey(b)))
    : tree.pins;
  const pins = ordered.flatMap((pin): Resolved[] => {
    const isSelected = (kind: string, id: number) =>
      selection?.kind === kind && "id" in selection && selection.id === id;
    if (pin.kind === "project") {
      const project = tree.projects.find((p) => p.id === pin.id);
      return project
        ? [
            {
              pin,
              label: project.name,
              icon: Box,
              location: "",
              open: () => actions.selectProject(project.id),
              selected: isSelected("project", project.id),
            },
          ]
        : [];
    }
    if (pin.kind === "folder") {
      const folder = tree.folders.find((f) => f.id === pin.id);
      return folder
        ? [
            {
              pin,
              label: folder.name,
              icon: Folder,
              location: projectName(folder.projectId),
              open: () => actions.selectFolder(folder.id),
              selected: isSelected("folder", folder.id),
            },
          ]
        : [];
    }
    const item = tree.items.find((i) => i.id === pin.id);
    return item
      ? [
          {
            pin,
            label: item.title,
            icon: itemIcon(item),
            location: projectName(item.projectId),
            open: () => actions.openItem(item),
            selected: isSelected("item", item.id),
          },
        ]
      : [];
  });

  const order = pins.map((p) => pinKey(p.pin));

  // Unpinned rows stay a moment at their old position and close (the reverse of rows growing
  // in), whichever screen did the unpinning.
  const [exiting, setExiting] = useState<{ entry: Resolved; index: number }[]>([]);
  const previous = useRef<Resolved[]>(pins);
  const orderKey = order.join(",");
  useEffect(() => {
    const current = new Set(orderKey.split(","));
    const gone = previous.current
      .map((entry, index) => ({ entry, index }))
      .filter(({ entry }) => !current.has(pinKey(entry.pin)));
    previous.current = pins;
    if (gone.length === 0) return;
    const keys = new Set(gone.map(({ entry }) => pinKey(entry.pin)));
    setExiting((e) => [...e, ...gone]);
    setTimeout(
      () => setExiting((e) => e.filter(({ entry }) => !keys.has(pinKey(entry.pin)))),
      motion().baseMs,
    );
    // `pins` is rebuilt every render; the key string is what actually changes.
  }, [orderKey]);

  const rows: { entry: Resolved; leaving: boolean }[] = pins.map((entry) => ({
    entry,
    leaving: false,
  }));
  for (const { entry, index } of exiting) {
    if (order.includes(pinKey(entry.pin))) continue; // pinned again meanwhile
    rows.splice(Math.min(index, rows.length), 0, { entry, leaving: true });
  }
  const from = dragging === null ? -1 : order.indexOf(dragging);
  const to = over === null ? -1 : order.indexOf(over);

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    setDragging(null);
    setOver(null);
    const a = order.indexOf(String(active.id));
    const b = over ? order.indexOf(String(over.id)) : -1;
    if (a === -1 || b === -1 || a === b) return;
    const next = pins.map((p) => p.pin);
    const [moved] = next.splice(a, 1);
    next.splice(b, 0, moved);
    setOptimistic(next.map(pinKey));
    void actions.reorderPins(next).finally(() => setOptimistic(null));
  };

  if (rows.length === 0) return <p className={styles.empty}>No pins</p>;

  // The overlay keeps showing the last dragged pin while it lands.
  const lastActive = useRef<Resolved | null>(null);
  const active = pins.find((p) => pinKey(p.pin) === dragging) ?? null;
  if (active) lastActive.current = active;
  const ghost = active ?? lastActive.current;

  // The ghost glides into the slot while the real row stays hidden, then they swap.
  const { baseMs, easing } = motion();
  const dropAnimation: DropAnimation = {
    duration: baseMs,
    easing,
    // Lifted surface melts into the row's own background as it lands.
    keyframes: ({ transform: { initial, final } }) => {
      const css = getComputedStyle(document.documentElement);
      return [
        {
          transform: `translate3d(${initial.x}px, ${initial.y}px, 0)`,
          backgroundColor: css.getPropertyValue("--color-bg-elevated"),
          boxShadow: css.getPropertyValue("--shadow-popover"),
        },
        {
          transform: `translate3d(${final.x}px, ${final.y}px, 0)`,
          backgroundColor: "transparent",
          boxShadow: "none",
        },
      ];
    },
    sideEffects: defaultDropAnimationSideEffects({ styles: { active: { opacity: "0" } } }),
  };

  // Only rows that are new to the list grow in; rows that just moved stay put.
  const seen = useRef(new Set<string>());
  useEffect(() => {
    seen.current = new Set(order);
  });
  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragStart={({ active }) => setDragging(String(active.id))}
      onDragOver={({ over }) => setOver(over ? String(over.id) : null)}
      onDragEnd={onDragEnd}
      onDragCancel={() => {
        setDragging(null);
        setOver(null);
      }}
    >
      <div role="list">
        {rows.map(({ entry, leaving }) => (
          <PinnedRow
            key={pinKey(entry.pin)}
            entry={entry}
            leaving={leaving}
            settled={seen.current.has(pinKey(entry.pin))}
            // The insertion line shows on the side the row will land.
            insert={
              pinKey(entry.pin) === over && from !== -1 && from !== to
                ? from < to
                  ? "after"
                  : "before"
                : null
            }
            onMenu={(event) => {
              event.preventDefault();
              event.stopPropagation();
              setMenu({ anchor: { x: event.clientX, y: event.clientY }, pin: entry.pin });
            }}
            onUnpin={() => actions.setPinned(entry.pin, false)}
          />
        ))}
      </div>
      <DragOverlay dropAnimation={dropAnimation} className={styles.pinOverlay}>
        {ghost && (
          // A copy of the row itself: lifted while dragging, flat again as it lands, so it
          // matches the real row exactly when they swap.
          <div className={cx(styles.pinGhost, !active && styles.landing)}>
            <TreeRow
              depth={0}
              icon={ghost.icon}
              label={ghost.label}
              selected={ghost.selected}
              trailing={ghost.location || undefined}
            />
          </div>
        )}
      </DragOverlay>
      {menu && (
        <Menu
          anchor={menu.anchor}
          items={[
            { label: "Unpin", icon: PinOff, onSelect: () => actions.setPinned(menu.pin, false) },
          ]}
          onClose={() => setMenu(null)}
        />
      )}
    </DndContext>
  );
}

type RowProps = {
  entry: Resolved;
  leaving: boolean;
  settled: boolean;
  insert: "before" | "after" | null;
  onMenu: (event: MouseEvent) => void;
  onUnpin: () => void;
};

function PinnedRow({ entry, leaving, settled, insert, onMenu, onUnpin }: RowProps) {
  const id = pinKey(entry.pin);
  const draggable = useDraggable({ id });
  const droppable = useDroppable({ id });
  return (
    <div className={cx(styles.pinRow, insert && styles[insert])}>
      <TreeRow
        depth={0}
        icon={entry.icon}
        label={entry.label}
        settled={settled}
        selected={entry.selected}
        trailing={entry.location || undefined}
        onClick={entry.open}
        onContextMenu={onMenu}
        dnd={{
          id,
          ref: (el) => {
            draggable.setNodeRef(el);
            droppable.setNodeRef(el);
          },
          handleProps: draggable.listeners,
          isDragging: draggable.isDragging,
          leaving,
        }}
        actions={<IconButton icon={PinOff} label="Unpin" size="sm" onClick={onUnpin} />}
      />
    </div>
  );
}
