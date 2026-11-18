/** A stand-in for the system clipboard, for tests that check what was copied. */
export const clipboardWrite = vi.fn<(text: string) => Promise<void>>(() => Promise.resolve());

/** Routes `navigator.clipboard.writeText` (behind `copyText` in @roost/ui) to `clipboardWrite`. */
export function installClipboard(): void {
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText: clipboardWrite },
    configurable: true,
  });
}
