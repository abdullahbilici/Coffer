import { useCallback, useEffect, useState } from "react";
import type { Copy, Run } from "../app/actions";
import { api, type Project } from "../lib/api";
import { useSecrets } from "./useSecrets";
import type { Session } from "./useSession";

export type Overview = ReturnType<typeof useProjectOverview>;

type Options = {
  projectId: number;
  run: Run;
  session: Session;
  onCopy: Copy;
};

/**
 * Loads and edits a project's markdown overview, with the same chips and secret handling as
 * note bodies. The edit button lives in the page header.
 */
export function useProjectOverview({ projectId, run, session, onCopy }: Options) {
  const [project, setProject] = useState<Project | null>(null);
  const [draft, setDraft] = useState<string | null>(null);
  const secrets = useSecrets({
    readable: project?.readme ?? "",
    protected: project?.hasSecrets ?? false,
    wholeBody: false,
    reveal: () => api.revealProjectReadme(projectId),
    session,
    run,
    onCopy,
  });
  const { forget } = secrets;

  const load = useCallback(
    () =>
      run(async () => {
        setProject(await api.getProject(projectId));
        forget();
      }),
    [projectId, run, forget],
  );

  useEffect(() => {
    void load();
  }, [load]);

  const startEdit = (fallback: string) =>
    run(async () => {
      await session.ensureUnlocked();
      setDraft((await secrets.full()) || fallback);
    });

  const save = () =>
    run(async () => {
      if (draft === null) return;
      await session.withUnlock(() => api.updateProjectReadme(projectId, draft));
      setDraft(null);
      await load();
    });

  return {
    readme: project?.readme ?? null,
    draft,
    setDraft,
    startEdit,
    save,
    chips: secrets.chips,
  };
}
