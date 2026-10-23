import { describe, expect, it } from "vitest";
import { keyLabel, shortcutFromEvent, shortcutParts } from "./shortcut";

const press = (
  code: string,
  mods: Partial<Record<"meta" | "ctrl" | "alt" | "shift", boolean>>,
) => ({
  code,
  metaKey: mods.meta ?? false,
  ctrlKey: mods.ctrl ?? false,
  altKey: mods.alt ?? false,
  shiftKey: mods.shift ?? false,
});

describe("shortcutFromEvent", () => {
  it("joins held modifiers in macOS order with the physical key", () => {
    expect(shortcutFromEvent(press("Space", { shift: true, alt: true }))).toBe("Alt+Shift+Space");
    expect(shortcutFromEvent(press("KeyN", { meta: true, ctrl: true }))).toBe("Control+Super+KeyN");
  });

  it("waits while only modifiers are down or the key can't be used", () => {
    expect(shortcutFromEvent(press("AltLeft", { alt: true }))).toBeNull();
    expect(shortcutFromEvent(press("CapsLock", { alt: true }))).toBeNull();
  });

  it("needs Command, Control or Option", () => {
    expect(shortcutFromEvent(press("KeyA", {}))).toBeNull();
    expect(shortcutFromEvent(press("KeyA", { shift: true }))).toBeNull();
  });
});

describe("shortcutParts", () => {
  it("reads a stored shortcut back for display", () => {
    expect(shortcutParts("Alt+Shift+Space")).toEqual({ modifiers: ["Alt", "Shift"], key: "Space" });
    expect(shortcutParts("Super+Control+Digit1")).toEqual({
      modifiers: ["Control", "Super"],
      key: "1",
    });
  });

  it("labels keys like their caps", () => {
    expect(keyLabel("KeyQ")).toBe("Q");
    expect(keyLabel("Slash")).toBe("/");
    expect(keyLabel("F12")).toBe("F12");
    expect(keyLabel("F21")).toBeNull();
  });
});
