import { useRef } from "react";
import type { Overview } from "../../hooks/useProjectOverview";
import { handleEditorKeys, handleMarkupKeys } from "../../lib/textarea";
import { Button } from "../ui/Button";
import { TextArea } from "../ui/Input";
import { Markdown } from "./Markdown";
import { MarkupToolbar } from "./MarkupToolbar";
import styles from "./ProjectOverview.module.css";

/** The overview, rendered full width above the project contents. */
export function ProjectOverview({ overview }: { overview: Overview }) {
  const { readme, draft, setDraft, save, chips } = overview;
  const editorRef = useRef<HTMLTextAreaElement>(null);

  if (draft !== null) {
    return (
      <div className={styles.editor}>
        <MarkupToolbar target={editorRef} secrets />
        <TextArea
          ref={editorRef}
          className={styles.area}
          value={draft}
          autoFocus
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) =>
            handleMarkupKeys(e, { secrets: true }) ||
            handleEditorKeys(e, { save: () => void save(), cancel: () => setDraft(null) })
          }
        />
        <div className={styles.footer}>
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
