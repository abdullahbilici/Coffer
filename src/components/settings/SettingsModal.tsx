import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { DatabaseBackup, type LucideIcon, SlidersHorizontal, Sparkles, X } from "lucide-react";
import type { Copy, Run } from "../../app/actions";
import type { Appearance } from "../../hooks/useAppearance";
import type { Session } from "../../hooks/useSession";
import { useSettings } from "../../hooks/useSettings";
import { IconButton } from "../ui/IconButton";
import { BackupSection } from "./BackupSection";
import { GeneralSection } from "./GeneralSection";
import styles from "./SettingsModal.module.css";

type Section = "general" | "backup" | "assistant";

const SECTIONS: { id: Section; label: string; icon: LucideIcon }[] = [
  { id: "general", label: "General", icon: SlidersHorizontal },
  { id: "backup", label: "Backup", icon: DatabaseBackup },
  { id: "assistant", label: "Assistant", icon: Sparkles },
];

type Props = {
  session: Session;
  run: Run;
  onCopy: Copy;
  appearance: Appearance;
  onAppearanceChange: (appearance: Appearance) => void;
  /** After a restore replaced the vault. */
  onRestored: () => Promise<void>;
  onClose: () => void;
};

/** Sections on the left, the chosen one on the right. Esc or the backdrop closes it. */
export function SettingsModal(props: Props) {
  const { session, run, onClose } = props;
  const [section, setSection] = useState<Section>("general");
  const { settings, setSettings } = useSettings(run);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onClose();
    };
    // Capture phase, like `Menu`: runs before the app-wide Esc handler clears the main pane.
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [onClose]);

  const title = SECTIONS.find((s) => s.id === section)?.label;

  return createPortal(
    <div
      className={styles.backdrop}
      onPointerDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className={styles.panel} role="dialog" aria-modal="true" aria-label="Settings">
        <nav className={styles.nav}>
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              className={styles.navItem}
              aria-current={id === section}
              onClick={() => setSection(id)}
            >
              <Icon size={16} strokeWidth={1.75} />
              {label}
            </button>
          ))}
        </nav>
        <section className={styles.content}>
          <header className={styles.header}>
            <h2 className={styles.title}>{title}</h2>
            <IconButton icon={X} label="Close" onClick={onClose} />
          </header>
          <div className={styles.body} key={section}>
            {section === "general" && settings && (
              <GeneralSection
                settings={settings}
                onChange={setSettings}
                session={session}
                run={run}
                appearance={props.appearance}
                onAppearanceChange={props.onAppearanceChange}
              />
            )}
            {section === "backup" && settings && (
              <BackupSection
                backup={settings.backup}
                onChange={(backup) => setSettings({ ...settings, backup })}
                session={session}
                run={run}
                onCopy={props.onCopy}
                onRestored={props.onRestored}
              />
            )}
            {section === "assistant" && <p className={styles.empty}>Coming soon</p>}
          </div>
        </section>
      </div>
    </div>,
    document.body,
  );
}
