import { describe, expect, it } from "vitest";
import cases from "../../fixtures/markup.json";
import { asMarker, secretValues, spanExit, toPlainText } from "./markup";

// Shared with `src-tauri/src/secrets.rs`, so both parsers agree on every case.
describe("markup fixtures", () => {
  for (const c of cases) {
    it(c.name, () => {
      expect(secretValues(c.body)).toEqual(c.secrets);
      expect(toPlainText(c.body)).toBe(c.plain);
      // The template the backend stores has one marker per secret, numbered in order.
      const markers = [...c.template.matchAll(/`([^`\n]+)`/g)]
        .map((m) => asMarker(m[1]))
        .filter((i) => i !== null);
      expect(markers).toEqual(c.secrets.map((_, i) => i));
    });
  }
});

describe("spanExit", () => {
  it("jumps past the closing backtick of a copyable or secret span", () => {
    expect(spanExit("a `bc` d", 4)).toBe(6);
    expect(spanExit("a `!bc!` d", 5)).toBe(8);
  });

  it("does nothing outside a span", () => {
    expect(spanExit("a `bc` d", 7)).toBeNull();
    expect(spanExit("plain", 2)).toBeNull();
  });

  it("stays on the current line", () => {
    expect(spanExit("`a\nb`", 2)).toBeNull();
  });
});
