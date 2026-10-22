import { Monitor, Moon, Sun } from "lucide-react";
import type { Run } from "../../app/actions";
import type { Appearance } from "../../hooks/useAppearance";
import type { Session } from "../../hooks/useSession";
import { api, type Settings } from "../../lib/api";
import { Segmented } from "../ui/Segmented";
import { Select } from "../ui/Select";
import { SettingRow } from "./SettingRow";

const UNLOCK_WINDOW_OPTIONS = [
  { value: 60, label: "1 minute" },
  { value: 5 * 60, label: "5 minutes" },
  { value: 10 * 60, label: "10 minutes" },
  { value: 30 * 60, label: "30 minutes" },
  { value: 60 * 60, label: "1 hour" },
  { value: null, label: "Until screen locks" },
];

const CLIPBOARD_OPTIONS = [
  { value: 30, label: "After 30 seconds" },
  { value: 60, label: "After 1 minute" },
  { value: null, label: "Never" },
];

const APPEARANCE_OPTIONS: { value: Appearance; label: string; icon: typeof Sun }[] = [
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
  { value: "system", label: "System", icon: Monitor },
];

type Props = {
  settings: Settings;
  onChange: (settings: Settings) => void;
  session: Session;
  run: Run;
  appearance: Appearance;
  onAppearanceChange: (appearance: Appearance) => void;
};

export function GeneralSection({ settings, onChange, session, run, ...props }: Props) {
  // The backend restarts the open window with the new length.
  const setUnlockWindow = (secs: number | null) =>
    run(async () => {
      session.update(await session.withUnlock(() => api.setUnlockWindow(secs)));
      onChange({ ...settings, unlockWindowSecs: secs });
    });

  const setClipboardClear = (secs: number | null) =>
    run(async () => {
      await session.withUnlock(() => api.setClipboardClear(secs));
      onChange({ ...settings, clipboardClearSecs: secs });
    });

  return (
    <>
      <SettingRow label="Unlock window">
        <Select
          label="Unlock window"
          value={settings.unlockWindowSecs}
          options={UNLOCK_WINDOW_OPTIONS}
          onChange={setUnlockWindow}
        />
      </SettingRow>
      <SettingRow label="Clear copied secrets">
        <Select
          label="Clear copied secrets"
          value={settings.clipboardClearSecs}
          options={CLIPBOARD_OPTIONS}
          onChange={setClipboardClear}
        />
      </SettingRow>
      <SettingRow label="Appearance">
        <Segmented
          value={props.appearance}
          options={APPEARANCE_OPTIONS}
          onChange={props.onAppearanceChange}
        />
      </SettingRow>
    </>
  );
}
