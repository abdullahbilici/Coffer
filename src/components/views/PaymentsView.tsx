import { Plus } from "lucide-react";
import type { VaultActions } from "../../app/actions";
import { useStoredState } from "../../hooks/useStoredState";
import type { Payment, Tree } from "../../lib/api";
import { addDays, addMonths, formatDate, formatMonth, monthStart, today } from "../../lib/dates";
import { cx } from "../../lib/cx";
import { formatMoney, priceOn, schedule, totals, type Due, type Totals } from "../../lib/schedule";
import { Button } from "../ui/Button";
import { Select } from "../ui/Select";
import { Tabs } from "../ui/Tabs";
import styles from "./PaymentsView.module.css";
import agendaStyles from "./Agenda.module.css";
import viewStyles from "./View.module.css";

type Tab = "schedule" | "spend" | "all";

const TABS: { value: Tab; label: string }[] = [
  { value: "schedule", label: "Schedule" },
  { value: "spend", label: "Spend" },
  { value: "all", label: "All" },
];

type Period = "month" | "year" | "lastYear" | "all";

const PERIODS: { value: Period; label: string }[] = [
  { value: "month", label: "This month" },
  { value: "year", label: "This year" },
  { value: "lastYear", label: "Last year" },
  { value: "all", label: "All time" },
];

/** How far ahead the schedule looks. */
const SCHEDULE_MONTHS = 12;

const CYCLE_SUFFIX = { monthly: " / month", yearly: " / year", once: "" };

type Props = { tree: Tree; actions: VaultActions };

/** What is due when, and what each project costs. Every past due date counts as paid. */
export function PaymentsView({ tree, actions }: Props) {
  const [tab, setTab] = useStoredState<Tab>("coffer.payments.tab", "schedule");
  const now = today();
  const projectName = (payment: Payment) =>
    tree.projects.find((p) => p.id === payment.projectId)?.name ?? "Personal";

  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={viewStyles.inner}>
        <header className={agendaStyles.header}>
          <h1 className={viewStyles.title}>Payments</h1>
          <div className={viewStyles.actions}>
            <Tabs label="View" value={tab} options={TABS} onChange={setTab} />
            <Button
              variant="ghost"
              size="sm"
              icon={Plus}
              onClick={() => actions.editPayment(null, null)}
            >
              New payment
            </Button>
          </div>
        </header>

        {tree.payments.length === 0 ? (
          <p className={agendaStyles.empty}>Empty</p>
        ) : tab === "schedule" ? (
          <Schedule tree={tree} now={now} projectName={projectName} actions={actions} />
        ) : tab === "spend" ? (
          <Spend tree={tree} now={now} />
        ) : (
          <AllPayments tree={tree} now={now} projectName={projectName} actions={actions} />
        )}
      </div>
    </div>
  );
}

type ListProps = {
  tree: Tree;
  now: string;
  projectName: (payment: Payment) => string;
  actions: VaultActions;
};

/** Upcoming charges, grouped by month with each month's totals. */
function Schedule({ tree, now, projectName, actions }: ListProps) {
  const dues = schedule(tree.payments, now, addMonths(monthStart(now), SCHEDULE_MONTHS));
  const months = new Map<string, Due[]>();
  for (const due of dues) {
    const month = monthStart(due.date);
    months.set(month, [...(months.get(month) ?? []), due]);
  }
  if (months.size === 0) return <p className={agendaStyles.empty}>Nothing due</p>;

  return [...months].map(([month, monthDues]) => (
    <section key={month} className={agendaStyles.group}>
      <h2 className={cx(agendaStyles.groupTitle, styles.monthTitle)}>
        {formatMonth(month)}
        <TotalsText totals={totals(monthDues)} />
      </h2>
      <ul className={styles.list}>
        {monthDues.map(({ payment, date, amount }) => (
          <li key={`${payment.id}:${date}`}>
            <button
              type="button"
              className={styles.row}
              onClick={() => actions.editPayment(payment.id, payment.projectId)}
            >
              <span className={styles.date}>{formatDate(date, now)}</span>
              <span className={styles.name}>{payment.name}</span>
              <span className={styles.secondary}>{projectName(payment)}</span>
              <span className={styles.amount}>{formatMoney(amount, payment.currency)}</span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  ));
}

function periodRange(period: Period, now: string): [string, string] {
  const year = Number(now.slice(0, 4));
  switch (period) {
    case "month":
      return [monthStart(now), now];
    case "year":
      return [`${year}-01-01`, now];
    case "lastYear":
      return [`${year - 1}-01-01`, `${year - 1}-12-31`];
    case "all":
      return ["0000-01-01", now];
  }
}

/** Paid so far per project (and personal), in each currency. */
function Spend({ tree, now }: { tree: Tree; now: string }) {
  const [period, setPeriod] = useStoredState<Period>("coffer.payments.period", "year");
  const [from, through] = periodRange(period, now);
  const dues = schedule(tree.payments, from, addDays(through, 1));
  const owners = [null, ...tree.projects.map((p) => p.id)];
  const rows = owners
    .map((projectId) => ({
      projectId,
      name: tree.projects.find((p) => p.id === projectId)?.name ?? "Personal",
      totals: totals(dues.filter((d) => d.payment.projectId === projectId)),
    }))
    .filter((row) => row.totals.length > 0);

  return (
    <>
      <div className={styles.toolbar}>
        <Select size="sm" label="Period" value={period} options={PERIODS} onChange={setPeriod} />
      </div>
      {rows.length === 0 ? (
        <p className={agendaStyles.empty}>Nothing paid</p>
      ) : (
        <ul className={styles.list}>
          {rows.map((row) => (
            <li key={row.projectId ?? "personal"} className={styles.row}>
              <span className={styles.name}>{row.name}</span>
              <TotalsText totals={row.totals} className={styles.amount} />
            </li>
          ))}
          <li className={cx(styles.row, styles.totalRow)}>
            <span className={styles.name}>Total</span>
            <TotalsText totals={totals(dues)} className={styles.amount} />
          </li>
        </ul>
      )}
    </>
  );
}

/** Every payment with its current price; cancelled ones last. */
function AllPayments({ tree, now, projectName, actions }: ListProps) {
  const ended = (p: Payment) =>
    (p.cancelDate !== null && p.cancelDate <= now) || (p.cycle === "once" && p.startDate < now);
  const payments = [...tree.payments].sort((a, b) => Number(ended(a)) - Number(ended(b)));

  return (
    <ul className={styles.list}>
      {payments.map((payment) => (
        <li key={payment.id}>
          <button
            type="button"
            className={cx(styles.row, ended(payment) && styles.ended)}
            onClick={() => actions.editPayment(payment.id, payment.projectId)}
          >
            <span className={styles.name}>{payment.name}</span>
            <span className={styles.secondary}>{projectName(payment)}</span>
            <span className={styles.secondary}>
              {payment.cancelDate !== null
                ? `${payment.cancelDate <= now ? "Ended" : "Ends"} ${formatDate(payment.cancelDate, now)}`
                : payment.cycle === "once"
                  ? formatDate(payment.startDate, now)
                  : ""}
            </span>
            <span className={styles.amount}>
              {formatMoney(priceOn(payment, now), payment.currency)}
              {CYCLE_SUFFIX[payment.cycle]}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

function TotalsText({ totals, className }: { totals: Totals; className?: string }) {
  return (
    <span className={cx(styles.totals, className)}>
      {totals.map(({ currency, amount }) => (
        <span key={currency}>{formatMoney(amount, currency)}</span>
      ))}
    </span>
  );
}
