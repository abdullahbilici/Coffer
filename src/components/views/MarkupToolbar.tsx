import type { RefObject } from "react";
import { Code, LockKeyhole, type LucideIcon } from "lucide-react";
import { COPYABLE, SECRET, type Span } from "../../lib/markup";
import { toggleSpan } from "../../lib/textarea";
import { Button } from "../ui/Button";
import { Kbd } from "../ui/Kbd";
import styles from "./MarkupToolbar.module.css";

type Props = {
  target: RefObject<HTMLTextAreaElement | null>;
  /** Off for fully secure items, where inline secrets make no sense. */
  secrets: boolean;
};

/** Copyable and Secret buttons above a markup-aware textarea. */
export function MarkupToolbar({ target, secrets }: Props) {
  const mark = (span: Span) => {
    if (target.current) toggleSpan(target.current, span);
  };
  return (
    <div className={styles.toolbar}>
      <MarkButton icon={Code} label="Copyable" letter="E" onClick={() => mark(COPYABLE)} />
      {secrets && (
        <MarkButton icon={LockKeyhole} label="Secret" letter="P" onClick={() => mark(SECRET)} />
      )}
    </div>
  );
}

type MarkButtonProps = {
  icon: LucideIcon;
  label: string;
  letter: string;
  onClick: () => void;
};

function MarkButton({ icon, label, letter, onClick }: MarkButtonProps) {
  return (
    <Button
      variant="ghost"
      size="sm"
      icon={icon}
      // Keep the textarea focused so its selection survives the click.
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      {label}
      <Kbd letter={letter} />
    </Button>
  );
}
