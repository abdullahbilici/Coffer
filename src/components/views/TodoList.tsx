import { useEffect, useState } from "react";
import { ArrowRight, ChevronDown, Circle, CircleCheck, FileText, Pencil } from "lucide-react";
import type { Copy, Run, VaultActions } from "../../app/actions";
import { useSecrets } from "../../hooks/useSecrets";
import type { Session } from "../../hooks/useSession";
import { api, type Todo, type Tree } from "../../lib/api";
import { cx } from "../../lib/cx";
import { formatDate, today } from "../../lib/dates";
import { agendaKey } from "../../lib/selection";
import { toggled } from "../../lib/sets";
import { Button } from "../ui/Button";
import { Collapse } from "../ui/Collapse";
import { DeleteButton } from "../ui/DeleteButton";
import { Markdown } from "./Markdown";
import styles from "./TodoList.module.css";

type Props = {
  todos: Todo[];
  tree: Tree;
  session: Session;
  run: Run;
  actions: VaultActions;
  onCopy: Copy;
  /** Row keys closing before they leave the list (`agendaKey.todo`). */
  deleting: ReadonlySet<string>;
};

/** Todos that expand in place into their description, linked note and actions. */
export function TodoList({ todos, deleting, ...rest }: Props) {
  const [open, setOpen] = useState<ReadonlySet<number>>(() => new Set());
  const now = today();
  return (
    <ul className={styles.list}>
      {todos.map((todo) => (
        <li key={todo.id}>
          <Collapse leaving={deleting.has(agendaKey.todo(todo.id))}>
            <TodoRow
              todo={todo}
              today={now}
              open={open.has(todo.id)}
              onToggle={() => setOpen((o) => toggled(o, todo.id))}
              {...rest}
            />
          </Collapse>
        </li>
      ))}
    </ul>
  );
}

type RowProps = Omit<Props, "todos" | "deleting"> & {
  todo: Todo;
  today: string;
  open: boolean;
  onToggle: () => void;
};

function TodoRow({ todo, today, open, onToggle, tree, actions, ...rest }: RowProps) {
  const note = tree.items.find((i) => i.id === todo.itemId);
  // Mounted on first open and kept, so collapsing animates instead of popping.
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    if (open) setLoaded(true);
  }, [open]);

  const Check = todo.done ? CircleCheck : Circle;
  const overdue = !todo.done && todo.dueDate !== null && todo.dueDate < today;

  return (
    <div className={cx(styles.entry, open && styles.entryOpen)}>
      {/* A div, not a button: the check inside is a button itself. */}
      <div
        role="button"
        tabIndex={0}
        className={styles.row}
        aria-expanded={open}
        onClick={onToggle}
        onKeyDown={(e) => {
          if (e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) {
            e.preventDefault();
            onToggle();
          }
        }}
      >
        <button
          type="button"
          className={styles.check}
          aria-label={todo.done ? "Mark as not done" : "Mark as done"}
          aria-pressed={todo.done}
          onClick={(e) => {
            e.stopPropagation();
            actions.setTodoDone(todo.id, !todo.done);
          }}
        >
          <Check size={16} strokeWidth={1.75} />
        </button>
        <span className={cx(styles.title, todo.done && styles.done)}>{todo.title}</span>
        {note && <FileText size={14} strokeWidth={1.75} className={styles.trail} />}
        {todo.dueDate && (
          <span className={cx(styles.due, overdue && styles.overdue)}>
            {todo.dueDate === today ? "Today" : formatDate(todo.dueDate, today)}
          </span>
        )}
        <ChevronDown size={16} strokeWidth={1.75} className={cx(styles.trail, styles.chevron)} />
      </div>
      <div className={styles.panel}>
        <div className={styles.panelInner}>
          {loaded && (
            <div className={styles.details}>
              {todo.description && <Description todo={todo} {...rest} />}
              <div className={styles.footer}>
                {note && (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={ArrowRight}
                    onClick={() => actions.openItem(note)}
                  >
                    {note.title}
                  </Button>
                )}
                <span className={styles.spacer} />
                <Button
                  variant="ghost"
                  size="sm"
                  icon={Pencil}
                  onClick={() => actions.editTodo(todo.id, todo.projectId)}
                >
                  Edit
                </Button>
                <DeleteButton onDelete={() => actions.deleteTodo(todo.id)} />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

type DescriptionProps = {
  todo: Todo;
  session: Session;
  run: Run;
  onCopy: Copy;
};

/** Rendered like a note body: markdown with chips, secrets masked until unhidden. */
function Description({ todo, session, run, onCopy }: DescriptionProps) {
  const { chips } = useSecrets({
    readable: todo.description,
    protected: todo.hasSecrets,
    wholeBody: false,
    reveal: () => api.revealTodo(todo.id),
    session,
    run,
    onCopy,
  });
  return <Markdown source={todo.description} chips={chips} size="md" />;
}
