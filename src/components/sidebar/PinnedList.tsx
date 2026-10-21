import { useEffect, useRef, useState, type MouseEvent } from "react";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  pointerWithin,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  defaultDropAnimationSideEffects,
  type DragEndEvent,
  type DragMoveEvent,
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

type Drop = { key: string; side: "before" | "after" };

/** The order after moving `dragged` to `drop`, or `null` if nothing would change. */
function reorder(order: string[], dragged: string, drop: Drop): string[] | null {
  if (dragged === drop.key || !order.includes(dragged)) return null;
  const rest = order.filter((key) => key !== dragged);
  const target = rest.indexOf(drop.key);
  if (target === -1) return null;
  rest.splice(drop.side === "before" ? target : target + 1, 0, dragged);
  return rest.join() === order.join() ? null : rest;
}

/** The dragged pin's tilt; matches `.pinGhost` in Sidebar.module.css. */
const LIFTED = "rotate(-1deg) scale(1.02)";

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
  /** The row edge nearest the pointer: the dragged pin goes before or after that row. */
  const [drop, setDrop] = useState<Drop | null>(null);
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
  // `pins` is rebuilt every render; read the latest through a ref and react to the key string.
  const latest = useRef(pins);
  latest.current = pins;
  const orderKey = order.join(",");
  useEffect(() => {
    const current = new Set(orderKey.split(","));
    const gone = previous.current
      .map((entry, index) => ({ entry, index }))
      .filter(({ entry }) => !current.has(pinKey(entry.pin)));
    previous.current = latest.current;
    if (gone.length === 0) return;
    const keys = new Set(gone.map(({ entry }) => pinKey(entry.pin)));
    setExiting((e) => [...e, ...gone]);
    setTimeout(
      () => setExiting((e) => e.filter(({ entry }) => !keys.has(pinKey(entry.pin)))),
      motion().baseMs,
    );
  }, [orderKey]);

  // Only rows that are new to the list grow in; rows that just moved stay put.
  const seen = useRef(new Set<string>());
  useEffect(() => {
    seen.current = new Set(orderKey.split(","));
  }, [orderKey]);

  const rows: { entry: Resolved; leaving: boolean }[] = pins.map((entry) => ({
    entry,
    leaving: false,
  }));
  for (const { entry, index } of exiting) {
    if (order.includes(pinKey(entry.pin))) continue; // pinned again meanwhile
    rows.splice(Math.min(index, rows.length), 0, { entry, leaving: true });
  }

  const track = ({ over, delta, activatorEvent }: DragMoveEvent) => {
    if (!over) return setDrop(null);
    const pointerY = (activatorEvent as PointerEvent).clientY + delta.y;
    const side = pointerY < over.rect.top + over.rect.height / 2 ? "before" : "after";
    const key = String(over.id);
    setDrop((d) => (d?.key === key && d.side === side ? d : { key, side }));
  };

  const onDragEnd = ({ active }: DragEndEvent) => {
    const next = dragging && drop ? reorder(order, String(active.id), drop) : null;
    setDragging(null);
    setDrop(null);
    if (!next) return;
    const byKey = new Map(pins.map((p) => [pinKey(p.pin), p.pin]));
    setOptimistic(next);
    void actions.reorderPins(next.map((key) => byKey.get(key)!)).finally(() => setOptimistic(null));
  };

  // Indicator only where dropping would actually change the order.
  const preview = dragging && drop && reorder(order, dragging, drop) ? drop : null;

  if (rows.length === 0) return <p className={styles.empty}>No pins</p>;

  const active = pins.find((p) => pinKey(p.pin) === dragging) ?? null;

  // The ghost glides into the slot while the real row stays hidden, then they swap.
  const { glideMs, easing } = motion();
  const dropAnimation: DropAnimation = {
    duration: glideMs,
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
    // dnd-kit freezes the overlay's children while it lands, so a CSS class can't flatten the
    // tilt; animate the inner surface directly, in step with the glide.
    sideEffects: (params) => {
      const restore = defaultDropAnimationSideEffects({ styles: { active: { opacity: "0" } } })(
        params,
      );
      params.dragOverlay.node.firstElementChild?.animate(
        [{ transform: LIFTED }, { transform: "none" }],
        { duration: glideMs, easing, fill: "forwards" },
      );
      return restore;
    },
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={pointerWithin}
      onDragStart={({ active }) => setDragging(String(active.id))}
      onDragMove={track}
      onDragOver={track}
      onDragEnd={onDragEnd}
      onDragCancel={() => {
        setDragging(null);
        setDrop(null);
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
            insert={preview?.key === pinKey(entry.pin) ? preview.side : null}
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
        {active && (
          // A copy of the row itself: lifted while dragging, flat again as it lands, so it
          // matches the real row exactly when they swap.
          <div className={styles.pinGhost}>
            <TreeRow
              depth={0}
              icon={active.icon}
              label={active.label}
              selected={active.selected}
              trailing={active.location || undefined}
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
