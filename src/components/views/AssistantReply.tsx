import { useMemo, useState } from "react";
import type { Copy, Run, VaultActions } from "../../app/actions";
import { useHandleChips } from "../../hooks/useHandleChips";
import type { Session } from "../../hooks/useSession";
import type { Tree } from "../../lib/api";
import { handleSource, withSecretChips, type ReplySource } from "../../lib/assistant";
import { Markdown } from "./Markdown";
import { ReplySources } from "./ReplySources";
import styles from "./Assistant.module.css";

type Props = {
  text: string;
  session: Session;
  run: Run;
  onCopy: Copy;
  /** Where the answer came from, shown under it as links. */
  sources?: { list: ReplySource[]; tree: Tree; actions: VaultActions };
};

/**
 * Assistant markdown; secret handles render as masked chips the user can reveal, holding the
 * vault's value. Pointing at a chip marks the source it came from.
 */
export function AssistantReply({ text, session, run, onCopy, sources }: Props) {
  const { source, handles } = useMemo(() => withSecretChips(text), [text]);
  const chips = useHandleChips(handles, session, run, onCopy);
  const [hovered, setHovered] = useState<number | null>(null);
  const active = hovered === null || !sources ? null : handleSource(handles[hovered], sources.tree);
  return (
    <div className={styles.reply}>
      <Markdown source={source} chips={{ ...chips, onHoverSecret: setHovered }} size="md" />
      {sources && (
        <ReplySources
          sources={sources.list}
          tree={sources.tree}
          actions={sources.actions}
          active={active}
        />
      )}
    </div>
  );
}
