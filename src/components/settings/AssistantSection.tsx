import { useEffect, useState } from "react";
import { X } from "lucide-react";
import type { Run } from "../../app/actions";
import type { Session } from "../../hooks/useSession";
import { api, errorMessage, type AssistantSettings } from "../../lib/api";
import { Button } from "../ui/Button";
import { IconButton } from "../ui/IconButton";
import { Input, TextArea } from "../ui/Input";
import { Select } from "../ui/Select";
import { SettingRow } from "./SettingRow";
import styles from "./Settings.module.css";

const KEEP_ALIVE_OPTIONS = [
  { value: 30 * 60, label: "30 minutes" },
  { value: 60 * 60, label: "1 hour" },
  { value: 4 * 60 * 60, label: "4 hours" },
  { value: null, label: "While Coffer is open" },
];

type Check =
  { state: "checking" } | { state: "ok"; models: string[] } | { state: "failed"; message: string };

type Props = {
  assistant: AssistantSettings;
  onChange: (assistant: AssistantSettings) => void;
  session: Session;
  run: Run;
};

/** The model server, the model, an optional API key and what the assistant should know. */
export function AssistantSection({ assistant: saved, onChange, session, run }: Props) {
  const [url, setUrl] = useState(saved.url);
  const [model, setModel] = useState(saved.model);
  const [context, setContext] = useState(saved.context);
  const [keepAlive, setKeepAlive] = useState(saved.keepAliveSecs);
  /** A newly typed key; empty keeps the stored one. */
  const [apiKey, setApiKey] = useState("");
  const [removeKey, setRemoveKey] = useState(false);
  const [check, setCheck] = useState<Check | null>(null);
  const [busy, setBusy] = useState(false);

  const checkConnection = async () => {
    setCheck({ state: "checking" });
    try {
      const models = await api.assistantModels(url, apiKey || null);
      setCheck({ state: "ok", models });
    } catch (error) {
      setCheck({ state: "failed", message: errorMessage(error) });
    }
  };

  useEffect(() => {
    void checkConnection();
    // Once, when the section opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const dirty =
    url.trim() !== saved.url ||
    model.trim() !== saved.model ||
    context.trim() !== saved.context ||
    keepAlive !== saved.keepAliveSecs ||
    apiKey !== "" ||
    removeKey;

  const save = () =>
    run(async () => {
      setBusy(true);
      try {
        const key = removeKey ? "" : apiKey || null;
        const next = await session.withUnlock(() =>
          api.saveAssistantSettings(url, model, context, key, keepAlive),
        );
        onChange(next.assistant);
        setUrl(next.assistant.url);
        setContext(next.assistant.context);
        setApiKey("");
        setRemoveKey(false);
      } finally {
        setBusy(false);
      }
    });

  const models = check?.state === "ok" ? check.models : [];
  const modelOptions = (models.includes(model) ? models : [model, ...models]).map((m) => ({
    value: m,
    label: m,
  }));
  const hasKey = saved.hasApiKey && !removeKey;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (dirty && !busy) void save();
      }}
    >
      <SettingRow label="Server">
        <Input
          className={styles.wideField}
          value={url}
          spellCheck={false}
          onChange={(e) => setUrl(e.target.value)}
        />
      </SettingRow>
      <SettingRow label="Model">
        {models.length > 0 ? (
          <Select label="Model" value={model} options={modelOptions} onChange={setModel} />
        ) : (
          <Input
            className={styles.wideField}
            value={model}
            onChange={(e) => setModel(e.target.value)}
          />
        )}
      </SettingRow>
      <SettingRow label="Keep model loaded">
        <Select
          label="Keep model loaded"
          value={keepAlive}
          options={KEEP_ALIVE_OPTIONS}
          onChange={setKeepAlive}
        />
      </SettingRow>
      <SettingRow label="API key">
        <Input
          className={styles.wideField}
          type="password"
          value={apiKey}
          placeholder={hasKey ? "Saved" : "None"}
          autoComplete="off"
          onChange={(e) => {
            setApiKey(e.target.value);
            setRemoveKey(false);
          }}
        />
        {hasKey && <IconButton icon={X} label="Remove key" onClick={() => setRemoveKey(true)} />}
      </SettingRow>
      <SettingRow label="Connection">
        <ConnectionStatus check={check} />
        <Button size="sm" disabled={check?.state === "checking"} onClick={checkConnection}>
          Check
        </Button>
      </SettingRow>
      <div className={styles.form}>
        <label className={styles.field}>
          <span className={styles.fieldLabel}>What the assistant should know</span>
          <TextArea
            className={styles.prose}
            rows={6}
            value={context}
            onChange={(e) => setContext(e.target.value)}
          />
        </label>
        <div className={styles.actions}>
          <Button type="submit" size="sm" variant="primary" disabled={!dirty || busy}>
            Save
          </Button>
        </div>
      </div>
    </form>
  );
}

function ConnectionStatus({ check }: { check: Check | null }) {
  switch (check?.state) {
    case undefined:
      return null;
    case "checking":
      return <span className={styles.value}>Checking…</span>;
    case "ok": {
      const count = check.models.length;
      return <span className={styles.value}>{`${count} model${count === 1 ? "" : "s"}`}</span>;
    }
    case "failed":
      return (
        <span className={styles.error} title={check.message}>
          {check.message}
        </span>
      );
  }
}
