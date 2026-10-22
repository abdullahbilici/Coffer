import { describe, expect, it } from "vitest";
import type { Payment, Todo } from "./api";
import { addMonths, monthStart } from "./dates";
import { dueDates, parseAmount, priceOn, schedule, totals } from "./schedule";
import { groupByProject, matchesFilter, type TodoFilter } from "./todos";

const payment = (over: Partial<Payment> = {}): Payment => ({
  id: 1,
  projectId: null,
  name: "Hosting",
  currency: "USD",
  cycle: "monthly",
  startDate: "2026-01-31",
  cancelDate: null,
  prices: [{ from: "2026-01-31", amount: 1000 }],
  createdAt: 0,
  updatedAt: 0,
  ...over,
});

describe("dates", () => {
  it("adds months without drifting off the anchor day", () => {
    expect(addMonths("2026-01-31", 1)).toBe("2026-02-28");
    expect(addMonths("2026-01-31", 2)).toBe("2026-03-31");
    expect(addMonths("2024-02-29", 12)).toBe("2025-02-28");
    expect(addMonths("2026-11-15", 3)).toBe("2027-02-15");
    expect(monthStart("2026-10-23")).toBe("2026-10-01");
  });
});

describe("schedule", () => {
  it("lists due dates per cycle up to the cancel date", () => {
    expect(dueDates(payment(), "2026-01-01", "2026-05-01")).toEqual([
      "2026-01-31",
      "2026-02-28",
      "2026-03-31",
      "2026-04-30",
    ]);
    expect(dueDates(payment({ cancelDate: "2026-03-31" }), "2026-03-01", "2027-01-01")).toEqual([]);
    expect(dueDates(payment({ cycle: "yearly" }), "2026-06-01", "2029-01-01")).toEqual([
      "2027-01-31",
      "2028-01-31",
    ]);
    expect(dueDates(payment({ cycle: "once" }), "2026-01-01", "2026-02-01")).toEqual([
      "2026-01-31",
    ]);
  });

  it("charges old periods at their old price", () => {
    const p = payment({
      prices: [
        { from: "2026-01-31", amount: 1000 },
        { from: "2026-03-01", amount: 1500 },
      ],
    });
    expect(priceOn(p, "2026-02-28")).toBe(1000);
    expect(priceOn(p, "2026-03-31")).toBe(1500);
    // A start date moved before the first price uses the first price.
    expect(priceOn(p, "2025-12-31")).toBe(1000);
    const dues = schedule([p], "2026-01-01", "2026-05-01");
    expect(dues.map((d) => d.amount)).toEqual([1000, 1000, 1500, 1500]);
  });

  it("totals per currency without converting", () => {
    const eur = payment({ id: 2, currency: "EUR", cycle: "once", startDate: "2026-02-01" });
    expect(totals(schedule([payment(), eur], "2026-01-01", "2026-03-01"))).toEqual([
      { currency: "EUR", amount: 1000 },
      { currency: "USD", amount: 2000 },
    ]);
  });

  it("parses typed amounts into hundredths", () => {
    expect(parseAmount("12.5")).toBe(1250);
    expect(parseAmount("1 200,99")).toBe(120099);
    expect(parseAmount("0.1")).toBe(10);
    expect(parseAmount("12.345")).toBeNull();
    expect(parseAmount("-3")).toBeNull();
    expect(parseAmount("")).toBeNull();
  });
});

describe("todos", () => {
  const todo = (id: number, over: Partial<Todo> = {}): Todo => ({
    id,
    projectId: null,
    itemId: null,
    title: `Todo ${id}`,
    description: "",
    hasSecrets: false,
    dueDate: null,
    done: false,
    createdAt: 0,
    updatedAt: 0,
    ...over,
  });
  const todos = [
    todo(1, { dueDate: "2026-10-20" }),
    todo(2, { dueDate: "2026-10-23", projectId: 7 }),
    todo(3, { dueDate: "2026-10-30" }),
    todo(4),
    todo(5, { dueDate: "2026-10-01", done: true, projectId: 8 }),
  ];
  const ids = (filter: TodoFilter) =>
    todos.filter((t) => matchesFilter(t, filter, "2026-10-23")).map((t) => t.id);

  it("filters by due date relative to today", () => {
    expect(ids("all")).toEqual([1, 2, 3, 4]);
    expect(ids("overdue")).toEqual([1]);
    expect(ids("today")).toEqual([2]);
    expect(ids("upcoming")).toEqual([3]);
    expect(ids("done")).toEqual([5]);
  });

  it("puts general todos first, then projects in order", () => {
    const projects = [
      { id: 8, name: "Beta" },
      { id: 7, name: "Acme" },
      { id: 9, name: "Empty" },
    ];
    const groups = groupByProject(todos, projects);
    expect(groups.map((g) => g.project?.name ?? null)).toEqual([null, "Beta", "Acme"]);
    expect(groups[0].todos.map((t) => t.id)).toEqual([1, 3, 4]);
  });
});
