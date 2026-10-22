import { useState, type ReactNode } from "react";
import type { VaultActions } from "../../app/actions";
import type { Cycle, Payment, SavePaymentInput, Tree } from "../../lib/api";
import { addMonths, formatDate, today } from "../../lib/dates";
import { amountText, dueDates, formatMoney, parseAmount, priceOn } from "../../lib/schedule";
import { handleEditorKeys } from "../../lib/textarea";
import { Button } from "../ui/Button";
import { DateInput } from "../ui/DateInput";
import { DeleteButton } from "../ui/DeleteButton";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import styles from "./ItemEditor.module.css";
import paymentStyles from "./PaymentEditor.module.css";
import viewStyles from "./View.module.css";

const CYCLES: { value: Cycle; label: string }[] = [
  { value: "monthly", label: "Monthly" },
  { value: "yearly", label: "Yearly" },
  { value: "once", label: "Once" },
];

type Draft = {
  projectId: number | null;
  name: string;
  amount: string;
  currency: string;
  cycle: Cycle;
  startDate: string;
  cancelDate: string | null;
  /** When an edited amount takes effect. */
  priceFrom: string;
};

type Props = {
  /** `null` creates a new payment. */
  paymentId: number | null;
  /** Preset project for a new payment. */
  projectId: number | null;
  tree: Tree;
  actions: VaultActions;
  onCancel: () => void;
};

/**
 * Create or edit a payment. Changing the amount of an existing one records a price change from
 * a chosen date, so earlier periods keep the old amount.
 */
export function PaymentEditor({ paymentId, projectId, tree, actions, onCancel }: Props) {
  const payment = tree.payments.find((p) => p.id === paymentId) ?? null;
  const now = today();
  const [draft, setDraft] = useState<Draft>(() =>
    payment ? toDraft(payment, now) : emptyDraft(tree, projectId, now),
  );
  const [saving, setSaving] = useState(false);

  const update = (patch: Partial<Draft>) => setDraft((d) => ({ ...d, ...patch }));
  const amount = parseAmount(draft.amount);
  const priceChanged = payment !== null && amount !== null && amount !== priceOn(payment, now);
  const canSave =
    draft.name.trim().length > 0 && amount !== null && /^[A-Za-z]{3}$/.test(draft.currency.trim());

  const save = async () => {
    if (!canSave || saving) return;
    const input: SavePaymentInput = {
      id: payment?.id ?? null,
      projectId: draft.projectId,
      name: draft.name,
      currency: draft.currency,
      cycle: draft.cycle,
      startDate: draft.startDate,
      cancelDate: draft.cycle === "once" ? null : draft.cancelDate,
      price:
        payment === null || priceChanged
          ? { from: payment === null ? draft.startDate : draft.priceFrom, amount }
          : null,
    };
    setSaving(true);
    try {
      await actions.savePayment(input);
    } catch {
      // The action reports the error; keep the draft open.
    } finally {
      setSaving(false);
    }
  };

  const projects = [
    { value: null, label: "Personal" },
    ...tree.projects.map((p) => ({ value: p.id as number | null, label: p.name })),
  ];

  return (
    <div
      className={viewStyles.view}
      onKeyDown={(e) => handleEditorKeys(e, { save: () => void save(), cancel: onCancel })}
    >
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <div className={viewStyles.eyebrow}>{payment ? "Edit payment" : "New payment"}</div>
        <input
          className={styles.title}
          value={draft.name}
          placeholder="Untitled"
          autoFocus
          spellCheck={false}
          onChange={(e) => update({ name: e.target.value })}
        />

        <div className={styles.fields}>
          <Field label="Project">
            <Select
              value={draft.projectId}
              options={projects}
              onChange={(projectId) => update({ projectId })}
            />
          </Field>
          <Field label="Amount">
            <div className={paymentStyles.amount}>
              <Input
                className={paymentStyles.amountInput}
                value={draft.amount}
                inputMode="decimal"
                placeholder="0.00"
                onChange={(e) => update({ amount: e.target.value })}
              />
              <Input
                className={paymentStyles.currency}
                value={draft.currency}
                maxLength={3}
                placeholder="USD"
                aria-label="Currency"
                onChange={(e) => update({ currency: e.target.value.toUpperCase() })}
              />
            </div>
          </Field>
          {priceChanged && (
            <Field label="From">
              <DateInput
                value={draft.priceFrom}
                clearable={false}
                onChange={(priceFrom) => priceFrom && update({ priceFrom })}
              />
            </Field>
          )}
          <Field label="Cycle">
            <Select value={draft.cycle} options={CYCLES} onChange={(cycle) => update({ cycle })} />
          </Field>
          <Field label={draft.cycle === "once" ? "Date" : "Start"}>
            <DateInput
              value={draft.startDate}
              clearable={false}
              onChange={(startDate) => startDate && update({ startDate })}
            />
          </Field>
          {draft.cycle !== "once" && (
            <Field label="Cancel">
              <DateInput
                value={draft.cancelDate}
                onChange={(cancelDate) => update({ cancelDate })}
              />
            </Field>
          )}
          {payment && payment.prices.length > 1 && (
            <Field label="Prices">
              <ul className={paymentStyles.history}>
                {[...payment.prices].reverse().map((price) => (
                  <li key={price.from}>
                    <span>{formatDate(price.from, now)}</span>
                    <span>{formatMoney(price.amount, payment.currency)}</span>
                  </li>
                ))}
              </ul>
            </Field>
          )}
        </div>

        <div className={styles.footer}>
          {payment && (
            <>
              <DeleteButton onDelete={() => actions.deletePayment(payment.id)} />
              <span className={paymentStyles.spacer} />
            </>
          )}
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="primary" disabled={!canSave || saving} onClick={save}>
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <span className={styles.label}>{label}</span>
      <div>{children}</div>
    </>
  );
}

function toDraft(payment: Payment, now: string): Draft {
  // A price change most likely starts with the next charge.
  const next = dueDates(payment, now, addMonths(now, 13))[0] ?? now;
  return {
    projectId: payment.projectId,
    name: payment.name,
    amount: amountText(priceOn(payment, now)),
    currency: payment.currency,
    cycle: payment.cycle,
    startDate: payment.startDate,
    cancelDate: payment.cancelDate,
    priceFrom: next,
  };
}

function emptyDraft(tree: Tree, projectId: number | null, now: string): Draft {
  // New payments default to the currency used most recently.
  const latest = [...tree.payments].sort((a, b) => b.createdAt - a.createdAt)[0];
  return {
    projectId,
    name: "",
    amount: "",
    currency: latest?.currency ?? "USD",
    cycle: "monthly",
    startDate: now,
    cancelDate: null,
    priceFrom: now,
  };
}
