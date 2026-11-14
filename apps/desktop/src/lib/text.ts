/** Anything with a UTF-16 start and end, such as a `StatementRange`. */
export type Span = { start: number; end: number };

/** 1-based line and column of a UTF-16 `offset` in `text`. */
export function lineAndColumn(text: string, offset: number): { line: number; column: number } {
  const before = text.slice(0, Math.max(0, Math.min(offset, text.length)));
  const lines = before.split("\n");
  const last = lines[lines.length - 1] ?? "";
  return { line: lines.length, column: last.length + 1 };
}

/**
 * The statement to run for a cursor at `cursor`: the one containing it, else the closest one
 * that ends before it (the cursor usually sits after the `;`), else the first one after it.
 */
export function statementAt<T extends Span>(ranges: T[], cursor: number): T | null {
  const containing = ranges.find((r) => cursor >= r.start && cursor <= r.end);
  if (containing) return containing;
  const before = ranges.filter((r) => r.end < cursor);
  return before[before.length - 1] ?? ranges.find((r) => r.start > cursor) ?? null;
}
