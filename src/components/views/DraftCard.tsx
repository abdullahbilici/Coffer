import { useState } from "react";
import {
  ArrowRight,
  Box,
  Check,
  ChevronRight,
  FileText,
  Folder,
  ListTodo,
  Minus,
  PenLine,
  Plus,
  Wallet,
  type LucideIcon,
} from "lucide-react";
import type { Copy, Run } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import {
  api,
  type DraftEntry,
  type DraftGroup,
  type DraftStatus,
  type DraftValue,
} from "../../lib/api";
import { cx } from "../../lib/cx";
import { Button } from "../ui/Button";
import { CopyChip } from "./CopyChip";
import { SecretChip } from "./SecretChip";
import styles from "./DraftCard.module.css";

const KIND_ICONS: Record<DraftEntry["kind"], LucideIcon> = {
  project: Box,
  folder: Folder,
  note: FileText,
  todo: ListTodo,
  payment: Wallet,
};

const ACTION_LABELS: Record<DraftEntry["action"], string> = {
  add: "New",
  edit: "Edit",
  rename: "Rename",
  move: "Move",
  delete: "Delete",
  done: "Done",
  cancel: "Cancel",
};

const SIGN_ICONS = { add: Plus, change: PenLine, remove: Minus };

/** Past this many field lines, notes start folded so a large import stays readable. */
const FOLD_AFTER_LINES = 14;

type Props = {
  /** Quick capture's version: the panel is the card, and the diff scrolls above its footer. */
  compact?: boolean;
  groups: DraftGroup[];
  status: DraftStatus;
  /** Still being drafted: Apply and Discard wait until it is stored. */
  building?: boolean;
  session: Session;
  run: Run;
  onCopy: Copy;
  onApply: () => Promise<void>;
  onDiscard: () => Promise<void>;
};

/** A drafted change set as a calm diff, grouped by project, with Apply and Discard. */
export function DraftCard({
  compact = false,
  groups,
  status,
  building = false,
  session,
  run,
  onCopy,
  onApply,
  onDiscard,
}: Props) {
  const [busy, setBusy] = useState(false);
  const secrets = useDraftSecrets(session, run, onCopy);
  const lines = groups.reduce((n, g) => n + g.entries.reduce((m, e) => m + e.lines.length, 0), 0);
  const changes = groups.reduce((n, g) => n + g.entries.length, 0) || groups.length;
  const fold = lines > FOLD_AFTER_LINES;

  const act = (action: () => Promise<void>) => async () => {
    setBusy(true);
    try {
      await action();
    } finally {
      setBusy(false);
    }
  };

  return (
    <section
      className={cx(
        styles.card,
        compact && styles.compact,
        status === "discarded" && styles.discarded,
      )}
    >
      <div className={styles.body}>
        {groups.map((group, g) => (
          <div key={`${g}:${group.project}`} className={styles.group}>
            <div className={styles.groupHead}>
              <Box size={14} strokeWidth={1.75} className={styles.groupIcon} />
              <span className={styles.project}>{group.project}</span>
              {group.new && <span className={styles.tag}>New</span>}
            </div>
            {group.entries.map((entry, e) => (
              <Entry
                key={e}
                entry={entry}
                inNewGroup={group.new}
                id={`${g}:${e}`}
                folded={fold && entry.lines.length > 0}
                secrets={secrets}
              />
            ))}
          </div>
        ))}
      </div>
      <footer className={styles.footer}>
        {status === "pending" ? (
          <>
            <span className={styles.count}>
              {changes === 1 ? "1 change" : `${changes} changes`}
            </span>
            <Button size="sm" variant="ghost" disabled={busy || building} onClick={act(onDiscard)}>
              Discard
            </Button>
            <Button
              size="sm"
              variant="primary"
              icon={Check}
              disabled={busy || building}
              onClick={act(onApply)}
            >
              Apply
            </Button>
          </>
        ) : (
          <span key={status} className={styles.status}>
            {status === "applied" && <Check size={14} strokeWidth={1.75} />}
            {status === "applied" ? "Applied" : "Discarded"}
          </span>
        )}
      </footer>
    </section>
  );
}

type Secrets = ReturnType<typeof useDraftSecrets>;

function Entry({
  entry,
  inNewGroup,
  id,
  folded,
  secrets,
}: {
  entry: DraftEntry;
  /** Everything in a new project is new; the group says so once. */
  inNewGroup: boolean;
  id: string;
  folded: boolean;
  secrets: Secrets;
}) {
  const [open, setOpen] = useState(!folded);
  const Icon = KIND_ICONS[entry.kind];
  const hasLines = entry.lines.length > 0;
  const head = (
    <>
      <Icon size={14} strokeWidth={1.75} className={styles.kindIcon} />
      {entry.action === "rename" && entry.detail ? (
        <span className={styles.titleRow}>
          <span className={cx(styles.detail, styles.old)}>{entry.detail}</span>
          <ArrowRight size={12} strokeWidth={1.75} className={styles.arrow} />
          <span className={styles.title}>{entry.title}</span>
        </span>
      ) : (
        <span className={styles.titleRow}>
          <span className={styles.title}>{entry.title}</span>
          {entry.detail && <span className={styles.detail}>{entry.detail}</span>}
        </span>
      )}
      {folded && hasLines && !open && (
        <span className={styles.fieldCount}>
          {entry.lines.length === 1 ? "1 field" : `${entry.lines.length} fields`}
        </span>
      )}
      {!(inNewGroup && entry.action === "add") && (
        <span className={cx(styles.action, entry.action === "delete" && styles.actionDanger)}>
          {ACTION_LABELS[entry.action]}
        </span>
      )}
      {folded && hasLines && (
        <ChevronRight
          size={12}
          strokeWidth={1.75}
          className={cx(styles.chevron, open && styles.chevronOpen)}
        />
      )}
    </>
  );
  return (
    <div className={styles.entry}>
      {folded && hasLines ? (
        <button
          type="button"
          className={cx(styles.entryHead, styles.entryToggle)}
          aria-expanded={open}
          onClick={() => setOpen((o) => !o)}
        >
          {head}
        </button>
      ) : (
        <div className={styles.entryHead}>{head}</div>
      )}
      {hasLines && open && (
        <div className={styles.lines}>
          {entry.lines.map((line, l) => {
            const Sign = SIGN_ICONS[line.sign];
            return (
              <div key={l} className={styles.line}>
                <Sign size={12} strokeWidth={1.75} className={cx(styles.sign, styles[line.sign])} />
                <span className={cx(styles.label, line.sign === "remove" && styles.removed)}>
                  {line.label}
                </span>
                {line.value && (
                  <span className={styles.value}>{secrets.render(`${id}:${l}`, line.value)}</span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

/**
 * Masked values of a draft. The user's own values are already in the thread; stored secrets
 * are fetched by handle on the first unhide or copy, and every revealed value is dropped once
 * the unlock window closes.
 */
function useDraftSecrets(session: Session, run: Run, onCopy: Copy) {
  const [fetched, setFetched] = useState<ReadonlyMap<string, string>>(() => new Map());
  const [shown, setShown] = useState<ReadonlySet<string>>(() => new Set());
  const unlocked = session.unlocked;
  const [seenUnlocked, setSeenUnlocked] = useState(unlocked);
  if (seenUnlocked !== unlocked) {
    setSeenUnlocked(unlocked);
    if (!unlocked) {
      setFetched(new Map());
      setShown(new Set());
    }
  }

  const value = async (key: string, v: DraftValue): Promise<string> => {
    if ("text" in v) return v.text;
    const known = fetched.get(key);
    if (known !== undefined) return known;
    const revealed = await session.withUnlock(() => api.revealHandle(v.handle));
    setFetched((prev) => new Map(prev).set(key, revealed));
    return revealed;
  };

  const render = (key: string, v: DraftValue) => {
    if ("text" in v && !v.secret) return <CopyChip text={v.text} onCopy={onCopy} />;
    const known = "text" in v ? v.text : (fetched.get(key) ?? null);
    return (
      <SecretChip
        value={known}
        shown={shown.has(key)}
        onToggle={() =>
          void run(async () => {
            if (!shown.has(key)) await value(key, v);
            setShown((prev) => {
              const next = new Set(prev);
              if (next.has(key)) next.delete(key);
              else next.add(key);
              return next;
            });
          })
        }
        onCopy={() => void run(async () => onCopy(await value(key, v), true))}
      />
    );
  };

  return { render };
}
