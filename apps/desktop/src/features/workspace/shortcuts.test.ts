import { matchShortcut } from "./shortcuts";

const keys = (
  key: string,
  mods: Partial<Record<"meta" | "ctrl" | "alt" | "shift", boolean>> = {},
  code = "",
) => ({
  key,
  code,
  metaKey: mods.meta ?? false,
  ctrlKey: mods.ctrl ?? false,
  altKey: mods.alt ?? false,
  shiftKey: mods.shift ?? false,
});

describe("matchShortcut", () => {
  it.each([
    [keys("t", { meta: true }), true, "new-tab"],
    [keys("w", { meta: true }), true, "close-tab"],
    [keys("t", { ctrl: true }), false, "new-tab"],
    [keys("Tab", { ctrl: true }), true, "next-tab"],
    [keys("Tab", { ctrl: true, shift: true }), true, "previous-tab"],
    [keys("ArrowRight", { meta: true, alt: true }), true, "next-tab"],
    [keys("ArrowLeft", { meta: true, alt: true }), true, "previous-tab"],
    [keys("®", { meta: true, alt: true }, "KeyR"), true, "focus-results"],
  ])("%o on mac=%s is %s", (event, mac, expected) => {
    expect(matchShortcut(event, mac)).toBe(expected);
  });

  it.each([
    [keys("t"), true],
    [keys("t", { ctrl: true }), true],
    [keys("t", { meta: true, shift: true }), true],
    [keys("x", { meta: true }), true],
  ])("ignores %o", (event, mac) => {
    expect(matchShortcut(event, mac)).toBeNull();
  });
});
