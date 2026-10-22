import type { DateString } from "./api";

const pad = (n: number) => String(n).padStart(2, "0");

/** A local `Date` as `YYYY-MM-DD`. */
export function toDateString(date: Date): DateString {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Local midnight of a `YYYY-MM-DD` day. */
export function parseDate(date: DateString): Date {
  const [year, month, day] = date.split("-").map(Number);
  return new Date(year, month - 1, day);
}

export function today(): DateString {
  return toDateString(new Date());
}

export function addDays(date: DateString, days: number): DateString {
  const d = parseDate(date);
  d.setDate(d.getDate() + days);
  return toDateString(d);
}

const daysInMonth = (year: number, monthIndex: number) =>
  new Date(year, monthIndex + 1, 0).getDate();

/**
 * `months` after `date`, on the same day of the month, clamped to the month's length
 * (Jan 31 + 1 month = Feb 28). Always measured from `date`, so the clamp never drifts.
 */
export function addMonths(date: DateString, months: number): DateString {
  const [year, month, day] = date.split("-").map(Number);
  const index = year * 12 + month - 1 + months;
  const y = Math.floor(index / 12);
  const m = index - y * 12;
  return `${y}-${pad(m + 1)}-${pad(Math.min(day, daysInMonth(y, m)))}`;
}

/** First day of the month `date` falls in. */
export const monthStart = (date: DateString): DateString => `${date.slice(0, 7)}-01`;

const sameYear = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });
const otherYear = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });
const monthFormat = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" });

/** "Oct 23", with the year only when it isn't this year's. */
export function formatDate(date: DateString, now: DateString = today()): string {
  const format = date.slice(0, 4) === now.slice(0, 4) ? sameYear : otherYear;
  return format.format(parseDate(date));
}

/** "October 2026". */
export const formatMonth = (date: DateString): string => monthFormat.format(parseDate(date));
