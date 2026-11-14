import { SHORTCUTS, matchShortcut, matchesBinding, shortcutLabel } from "./registry";

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
    [keys("k", { meta: true }), true, "palette"],
    [keys("k", { ctrl: true }), false, "palette"],
    [keys(",", { meta: true }), true, "settings"],
    [keys("?", { meta: true, shift: true }, "Slash"), true, "shortcuts"],
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

describe("shortcut registry", () => {
  it("shows keys the platform's way", () => {
    expect(shortcutLabel("new-tab", true)).toBe("⌘T");
    expect(shortcutLabel("new-tab", false)).toBe("Ctrl+T");
    expect(shortcutLabel("next-tab", true)).toBe("⌃Tab or ⌘⌥→");
    expect(shortcutLabel("run-script", false)).toBe("Ctrl+Shift+↩");
  });

  it("gives every shortcut a unique id and at least one key", () => {
    const ids = SHORTCUTS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(SHORTCUTS.every((s) => s.keys.length > 0)).toBe(true);
  });

  it("needs exactly the binding's modifiers", () => {
    expect(matchesBinding(keys("Enter", { meta: true }), "Mod-Enter", true)).toBe(true);
    expect(matchesBinding(keys("Enter", { meta: true, shift: true }), "Mod-Enter", true)).toBe(
      false,
    );
    expect(matchesBinding(keys("Enter", { ctrl: true }), "Mod-Enter", false)).toBe(true);
  });
});
