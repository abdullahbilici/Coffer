import { useMemo } from "react";
import type { Copy, Run } from "../../app/actions";
import { useHandleChips } from "../../hooks/useHandleChips";
import type { Session } from "../../hooks/useSession";
import { withSecretChips } from "../../lib/assistant";
import { Markdown } from "./Markdown";
import styles from "./Assistant.module.css";

type Props = { text: string; session: Session; run: Run; onCopy: Copy };

/** Assistant markdown; secret handles render as masked chips the user can reveal. */
export function AssistantReply({ text, session, run, onCopy }: Props) {
  const { source, handles } = useMemo(() => withSecretChips(text), [text]);
  const chips = useHandleChips(handles, session, run, onCopy);
  return (
    <div className={styles.reply}>
      <Markdown source={source} chips={chips} size="md" />
    </div>
  );
}
