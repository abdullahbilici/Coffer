import { useMemo, useRef, useState, type ReactNode } from "react";
import type { SaveItemInput, Tree } from "../../lib/api";
import { handleEditorKeys, handleMarkupKeys } from "../../lib/textarea";
import { folderOptions } from "../../lib/tree";
import { Button } from "../ui/Button";
import { TextArea } from "../ui/Input";
import { Select } from "../ui/Select";
import { Switch } from "../ui/Switch";
import { TagInput } from "../ui/TagInput";
import { MarkupToolbar } from "./MarkupToolbar";
import styles from "./ItemEditor.module.css";
import viewStyles from "./View.module.css";

type Props = {
  tree: Tree;
  initial: SaveItemInput;
  heading: string;
  /** Should reject on failure so the draft stays open. */
  onSave: (input: SaveItemInput) => Promise<void>;
  onCancel: () => void;
};

export function ItemEditor({ tree, initial, heading, onSave, onCancel }: Props) {
  const [draft, setDraft] = useState(initial);
  const [saving, setSaving] = useState(false);
  const bodyRef = useRef<HTMLTextAreaElement>(null);

  const folders = useMemo(() => folderOptions(tree, draft.projectId), [tree, draft.projectId]);
  const projects = tree.projects.map((p) => ({ value: p.id, label: p.name }));
  const allTags = useMemo(
    () => [...new Set(tree.items.flatMap((i) => i.tags))].sort((a, b) => a.localeCompare(b)),
    [tree.items],
  );

  const update = (patch: Partial<SaveItemInput>) => setDraft((d) => ({ ...d, ...patch }));
  const canSave = draft.title.trim().length > 0 && !saving;

  const save = async () => {
    if (!canSave) return;
    setSaving(true);
    try {
      await onSave(draft);
    } catch {
      // The caller reports the error; keep the draft open.
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
        <div className={viewStyles.eyebrow}>{heading}</div>
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
              onChange={(projectId) => update({ projectId, folderId: null })}
            />
          </Field>
          <Field label="Folder">
            <Select
              value={draft.folderId}
              options={folders.map((f) => ({ value: f.id, label: f.label }))}
              onChange={(folderId) => update({ folderId })}
            />
          </Field>
          <Field label="Tags">
            <TagInput
              value={draft.tags}
              suggestions={allTags}
              onChange={(tags) => update({ tags })}
            />
          </Field>
          <Field label="Security">
            <Switch
              checked={draft.secure}
              label="Secure"
              onChange={(secure) => update({ secure })}
            />
          </Field>
        </div>

        <MarkupToolbar target={bodyRef} secrets={!draft.secure} />
        <TextArea
          ref={bodyRef}
          className={styles.body}
          value={draft.body}
          onChange={(e) => update({ body: e.target.value })}
          onKeyDown={(e) => handleMarkupKeys(e, { secrets: !draft.secure })}
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

export function emptyDraft(projectId: number, folderId: number | null): SaveItemInput {
  return { id: null, projectId, folderId, title: "", body: "", secure: true, tags: [] };
}
