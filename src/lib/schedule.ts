import type { DateString, Payment } from "./api";
import { addMonths } from "./dates";

/** One scheduled charge. Every due date on or before today counts as paid. */
export type Due = { payment: Payment; date: DateString; amount: number };

/** Amounts per currency, never converted. */
export type Totals = { currency: string; amount: number }[];

const STEP_MONTHS = { monthly: 1, yearly: 12 } as const;

/** The amount due on `date`: the latest price that started on or before it. */
export function priceOn(payment: Payment, date: DateString): number {
  let amount = payment.prices[0]?.amount ?? 0;
  for (const price of payment.prices) {
    if (price.from <= date) amount = price.amount;
  }
  return amount;
}

/** Due dates of one payment within [from, to), stopping at its cancel date. */
export function dueDates(payment: Payment, from: DateString, to: DateString): DateString[] {
  const end = payment.cancelDate !== null && payment.cancelDate < to ? payment.cancelDate : to;
  if (payment.cycle === "once") {
    const date = payment.startDate;
    return date >= from && date < end ? [date] : [];
  }
  const step = STEP_MONTHS[payment.cycle];
  const dates: DateString[] = [];
  for (let n = 0; ; n += step) {
    const date = addMonths(payment.startDate, n);
    if (date >= end) break;
    if (date >= from) dates.push(date);
  }
  return dates;
}

/** Every charge of `payments` within [from, to), by date then name. */
export function schedule(payments: Payment[], from: DateString, to: DateString): Due[] {
  return payments
    .flatMap((payment) =>
      dueDates(payment, from, to).map((date) => ({
        payment,
        date,
        amount: priceOn(payment, date),
      })),
    )
    .sort((a, b) => a.date.localeCompare(b.date) || a.payment.name.localeCompare(b.payment.name));
}

/** Sums per currency, sorted by currency code. */
export function totals(dues: Due[]): Totals {
  const sums = new Map<string, number>();
  for (const { payment, amount } of dues) {
    sums.set(payment.currency, (sums.get(payment.currency) ?? 0) + amount);
  }
  return [...sums]
    .map(([currency, amount]) => ({ currency, amount }))
    .sort((a, b) => a.currency.localeCompare(b.currency));
}

const formats = new Map<string, Intl.NumberFormat>();

/** An amount in hundredths as local currency text, e.g. "$12.50". */
export function formatMoney(amount: number, currency: string): string {
  let format = formats.get(currency);
  if (!format) {
    format = new Intl.NumberFormat(undefined, { style: "currency", currency });
    formats.set(currency, format);
  }
  return format.format(amount / 100);
}

/** "12.50" for a hundredths amount, for editing. */
export const amountText = (amount: number): string => (amount / 100).toFixed(2);

/** Parses "12.5", "12,50" or "1 200" into hundredths; `null` if it isn't an amount. */
export function parseAmount(text: string): number | null {
  const normalized = text.replace(/\s/g, "").replace(",", ".");
  if (!/^\d+(\.\d{0,2})?$/.test(normalized)) return null;
  return Math.round(Number(normalized) * 100);
}
