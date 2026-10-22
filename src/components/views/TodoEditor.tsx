import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { Run, VaultActions } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { api, type SaveTodoInput, type Tree } from "../../lib/api";
import { handleEditorKeys, handleMarkupKeys } from "../../lib/textarea";
import { locationLabel } from "../../lib/tree";
import { Button } from "../ui/Button";
import { DateInput } from "../ui/DateInput";
import { TextArea } from "../ui/Input";
import { Select } from "../ui/Select";
import { MarkupToolbar } from "./MarkupToolbar";
import styles from "./ItemEditor.module.css";
import viewStyles from "./View.module.css";

type Props = {
  /** `null` creates a new todo. */
  todoId: number | null;
  /** Preset project for a new todo. */
  projectId: number | null;
  tree: Tree;
  session: Session;
  run: Run;
  actions: VaultActions;
  onCancel: () => void;
};

/** Create or edit a todo. Secrets in the description are revealed only to fill the editor. */
export function TodoEditor({ todoId, projectId, tree, session, run, actions, onCancel }: Props) {
  const [draft, setDraft] = useState<SaveTodoInput | null>(null);
  const [saving, setSaving] = useState(false);
  const descriptionRef = useRef<HTMLTextAreaElement>(null);

  // Loads once per opened todo: later tree reloads or session changes must not reset the draft.
  const load = useRef(() => {});
  load.current = () => {
    const todo = tree.todos.find((t) => t.id === todoId);
    if (!todo) {
      setDraft(emptyTodo(projectId));
      return;
    }
    void run(async () => {
      const description = todo.hasSecrets
        ? await session.withUnlock(() => api.revealTodo(todo.id))
        : todo.description;
      setDraft({
        id: todo.id,
        projectId: todo.projectId,
        itemId: todo.itemId,
        title: todo.title,
        description,
        dueDate: todo.dueDate,
      });
    });
  };
  useEffect(() => load.current(), [todoId]);

  const projects = [
    { value: null, label: "None" },
    ...tree.projects.map((p) => ({ value: p.id as number | null, label: p.name })),
  ];
  const draftProject = draft?.projectId ?? null;
  const notes = useMemo(
    () => [
      { value: null, label: "None" },
      ...tree.items
        .filter((i) => draftProject === null || i.projectId === draftProject)
        .map((i) => ({
          value: i.id as number | null,
          label: draftProject === null ? `${locationLabel(tree, i)} / ${i.title}` : i.title,
        })),
    ],
    [tree, draftProject],
  );

  if (!draft) return null;

  const update = (patch: Partial<SaveTodoInput>) => setDraft((d) => d && { ...d, ...patch });
  const canSave = draft.title.trim().length > 0 && !saving;

  const save = async () => {
    if (!canSave) return;
    setSaving(true);
    try {
      await actions.saveTodo(draft);
    } catch {
      // The action reports the error; keep the draft open.
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className={viewStyles.view}
      onKeyDown={(e) => handleEditorKeys(e, { save: () => void save(), cancel: onCancel })}
    >
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <div className={viewStyles.eyebrow}>{draft.id === null ? "New todo" : "Edit todo"}</div>
        <input
          className={styles.title}
          value={draft.title}
          placeholder="Untitled"
          autoFocus
          spellCheck={false}
          onChange={(e) => update({ title: e.target.value })}
        />

        <div className={styles.fields}>
          <Field label="Project">
            <Select
              value={draft.projectId}
              options={projects}
              onChange={(projectId) => {
                const note = tree.items.find((i) => i.id === draft.itemId);
                const keepNote = projectId === null || note?.projectId === projectId;
                update({ projectId, itemId: keepNote ? draft.itemId : null });
              }}
            />
          </Field>
          <Field label="Due">
            <DateInput value={draft.dueDate} onChange={(dueDate) => update({ dueDate })} />
          </Field>
          <Field label="Note">
            <Select
              value={draft.itemId}
              options={notes}
              onChange={(itemId) => update({ itemId })}
            />
          </Field>
        </div>

        <MarkupToolbar target={descriptionRef} secrets />
        <TextArea
          ref={descriptionRef}
          className={styles.body}
          value={draft.description}
          onChange={(e) => update({ description: e.target.value })}
          onKeyDown={(e) => handleMarkupKeys(e, { secrets: true })}
        />

        <div className={styles.footer}>
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="primary" disabled={!canSave} onClick={save}>
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <span className={styles.label}>{label}</span>
      <div>{children}</div>
    </>
  );
}

function emptyTodo(projectId: number | null): SaveTodoInput {
  return { id: null, projectId, itemId: null, title: "", description: "", dueDate: null };
}
