import { Box, Plus } from "lucide-react";
import type { Run, VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { useStoredState } from "../../hooks/useStoredState";
import type { Tree } from "../../lib/api";
import { today } from "../../lib/dates";
import { groupByProject, matchesFilter, type TodoFilter } from "../../lib/todos";
import { Button } from "../ui/Button";
import { Tabs } from "../ui/Tabs";
import { TodoList } from "./TodoList";
import styles from "./Agenda.module.css";
import viewStyles from "./View.module.css";

const FILTERS: { value: TodoFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "overdue", label: "Overdue" },
  { value: "today", label: "Today" },
  { value: "upcoming", label: "Upcoming" },
  { value: "done", label: "Done" },
];

type Props = {
  tree: Tree;
  session: Session;
  run: Run;
  actions: VaultActions;
  onCopy: (text: string) => Promise<void>;
  deleting: ReadonlySet<string>;
};

/** Every todo on one screen: general ones first, then one group per project. */
export function TodosView(props: Props) {
  const { tree, actions } = props;
  const [filter, setFilter] = useStoredState<TodoFilter>("coffer.todos.filter", "all");
  const now = today();
  const count = (f: TodoFilter) => tree.todos.filter((t) => matchesFilter(t, f, now)).length;
  const groups = groupByProject(
    tree.todos.filter((t) => matchesFilter(t, filter, now)),
    tree.projects,
  );

  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <header className={styles.header}>
          <h1 className={viewStyles.title}>Todos</h1>
          <div className={viewStyles.actions}>
            <Tabs
              label="Filter"
              value={filter}
              options={FILTERS.map((f) => ({ ...f, count: count(f.value) }))}
              onChange={setFilter}
            />
            <Button
              variant="ghost"
              size="sm"
              icon={Plus}
              onClick={() => actions.editTodo(null, null)}
            >
              New todo
            </Button>
          </div>
        </header>

        {groups.length === 0 && <p className={styles.empty}>Empty</p>}
        {groups.map(({ project, todos }) => (
          <section key={project?.id ?? "general"} className={styles.group}>
            {project ? (
              <button
                type="button"
                className={styles.groupTitle}
                onClick={() => actions.selectProject(project.id)}
              >
                <Box size={14} strokeWidth={1.75} />
                {project.name}
              </button>
            ) : (
              <h2 className={styles.groupTitle}>General</h2>
            )}
            <TodoList todos={todos} {...props} />
          </section>
        ))}
      </div>
    </div>
  );
}
