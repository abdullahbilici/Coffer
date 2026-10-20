import { createContext, useContext, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";
import { COPY_TAG, SECRET_TAG, remarkMarkup } from "../../lib/remarkMarkup";
import { CopyChip } from "./CopyChip";
import { SecretChip } from "./SecretChip";
import styles from "./Markdown.module.css";

export type MarkdownChips = {
  /** Decrypted secret values in marker order, or `null` while not fetched. */
  secrets: string[] | null;
  shown: ReadonlySet<number>;
  onToggleSecret: (index: number) => void;
  onCopySecret: (index: number) => void;
  onCopyText: (text: string) => void;
};

// Chip state goes through context so `components` stays stable; recreating it would remount
// every chip (and replay its animation) on each render.
const ChipsContext = createContext<MarkdownChips | null>(null);

function useChips(): MarkdownChips {
  const chips = useContext(ChipsContext);
  if (!chips) throw new Error("Markdown chips rendered outside <Markdown>");
  return chips;
}

function Copyable({ children }: { children?: ReactNode }) {
  const chips = useChips();
  return (
    <CopyChip className={styles.chip} text={String(children ?? "")} onCopy={chips.onCopyText} />
  );
}

function Secret({ index }: { index: number }) {
  const chips = useChips();
  return (
    <SecretChip
      className={styles.chip}
      value={chips.secrets?.[index] ?? null}
      shown={chips.shown.has(index)}
      onToggle={() => chips.onToggleSecret(index)}
      onCopy={() => chips.onCopySecret(index)}
    />
  );
}

// Links open in the default browser instead of navigating the app window.
function Link({ href, children }: { href?: string; children?: ReactNode }) {
  return (
    <a
      href={href}
      onClick={(event) => {
        event.preventDefault();
        if (href) void openUrl(href);
      }}
    >
      {children}
    </a>
  );
}

const components = { a: Link, [COPY_TAG]: Copyable, [SECRET_TAG]: Secret } as Components;
const remarkPlugins = [remarkGfm, remarkMarkup];

/** Renders GitHub-flavoured markdown with Coffer chips. Raw HTML is not rendered. */
export function Markdown({ source, chips }: { source: string; chips: MarkdownChips }) {
  return (
    <ChipsContext.Provider value={chips}>
      <div className={styles.prose}>
        <ReactMarkdown remarkPlugins={remarkPlugins} components={components}>
          {source}
        </ReactMarkdown>
      </div>
    </ChipsContext.Provider>
  );
}
