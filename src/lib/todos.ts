import type { DateString, ProjectSummary, Todo } from "./api";

export type TodoFilter = "all" | "overdue" | "today" | "upcoming" | "done";

/** Open todos only, except for `done`. Undated todos only show under `all`. */
export function matchesFilter(todo: Todo, filter: TodoFilter, today: DateString): boolean {
  if (filter === "done") return todo.done;
  if (todo.done) return false;
  const due = todo.dueDate;
  switch (filter) {
    case "all":
      return true;
    case "overdue":
      return due !== null && due < today;
    case "today":
      return due === today;
    case "upcoming":
      return due !== null && due > today;
  }
}

export type TodoGroup = { project: ProjectSummary | null; todos: Todo[] };

/** General todos first, then one group per project in the given order; empty groups dropped. */
export function groupByProject(todos: Todo[], projects: ProjectSummary[]): TodoGroup[] {
  const groups: TodoGroup[] = [null, ...projects].map((project) => ({
    project,
    todos: todos.filter((t) => t.projectId === (project?.id ?? null)),
  }));
  return groups.filter((g) => g.todos.length > 0);
}
