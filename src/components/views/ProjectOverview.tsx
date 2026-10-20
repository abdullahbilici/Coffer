import { useEffect, useMemo, useRef, useState } from "react";
import type { Run } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { api, type Project } from "../../lib/api";
import { secretValues } from "../../lib/markup";
import { handleMarkupKeys } from "../../lib/textarea";
import { Button } from "../ui/Button";
import { TextArea } from "../ui/Input";
import { Markdown, type MarkdownChips } from "./Markdown";
import { MarkupToolbar } from "./MarkupToolbar";
import styles from "./ContainerView.module.css";

export type Overview = ReturnType<typeof useProjectOverview>;

type Options = {
  projectId: number;
  run: Run;
  session: Session;
  onCopy: (text: string) => Promise<void>;
};

/**
 * Loads and edits a project's markdown overview, with the same chips and secret handling as
 * item bodies. The edit button lives in the page header.
 */
export function useProjectOverview({ projectId, run, session, onCopy }: Options) {
  const [project, setProject] = useState<Project | null>(null);
  const [draft, setDraft] = useState<string | null>(null);
  // Decrypted overview, fetched on the first unhide/copy. Lives only as long as this view.
  const [plaintext, setPlaintext] = useState<string | null>(null);
  const [shown, setShown] = useState<ReadonlySet<number>>(() => new Set());

  const load = useMemo(
    () => () =>
      run(async () => {
        setProject(await api.getProject(projectId));
        setPlaintext(null);
        setShown(new Set());
      }),
    [projectId, run],
  );

  useEffect(() => {
    void load();
  }, [load]);

  // Drop decrypted content as soon as the unlock window closes.
  useEffect(() => {
    if (session.unlocked) return;
    setPlaintext(null);
    setShown(new Set());
  }, [session.unlocked]);

  /** The full overview; asks for Touch ID only when the unlock window is closed. */
  const fullReadme = async (): Promise<string> => {
    if (!project) return "";
    if (!project.hasSecrets) return project.readme;
    if (plaintext !== null) return plaintext;
    const text = await session.withUnlock(() => api.revealProjectReadme(projectId));
    setPlaintext(text);
    return text;
  };

  const chips: MarkdownChips = {
    secrets: plaintext === null ? null : secretValues(plaintext),
    shown,
    onToggleSecret: (index) =>
      run(async () => {
        if (!shown.has(index)) await fullReadme();
        setShown((prev) => {
          const next = new Set(prev);
          if (!next.delete(index)) next.add(index);
          return next;
        });
      }),
    onCopySecret: (index) =>
      run(async () => {
        const value = secretValues(await fullReadme())[index];
        if (value !== undefined) await onCopy(value);
      }),
    onCopyText: (text) => void run(() => onCopy(text)),
  };

  const startEdit = (fallback: string) =>
    run(async () => setDraft((await fullReadme()) || fallback));

  const save = () =>
    run(async () => {
      if (draft === null) return;
      await session.withUnlock(() => api.updateProjectReadme(projectId, draft));
      setDraft(null);
      await load();
    });

  return { readme: project?.readme ?? null, draft, setDraft, startEdit, save, chips };
}

/** The overview, rendered full width above the project contents. */
export function ProjectOverview({ overview }: { overview: Overview }) {
  const { readme, draft, setDraft, save, chips } = overview;
  const editorRef = useRef<HTMLTextAreaElement>(null);

  if (draft !== null) {
    return (
      <div className={styles.overviewEditor}>
        <MarkupToolbar target={editorRef} secrets />
        <TextArea
          ref={editorRef}
          className={styles.overviewArea}
          value={draft}
          autoFocus
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (handleMarkupKeys(e, { secrets: true })) return;
            if (e.metaKey && e.key === "s") {
              e.preventDefault();
              void save();
            }
            if (e.key === "Escape") {
              e.preventDefault();
              setDraft(null);
            }
          }}
        />
        <div className={styles.overviewFooter}>
          <Button variant="ghost" onClick={() => setDraft(null)}>
            Cancel
          </Button>
          <Button variant="primary" onClick={save}>
            Save
          </Button>
        </div>
      </div>
    );
  }

  if (!readme) return null;
  return (
    <div className={styles.overview}>
      <Markdown source={readme} chips={chips} />
    </div>
  );
}
