/** What a run is made of, and how the statements to run are chosen (tasks 5.5, 5.6). Pure. */
import type { AppError, QueryResult, StatementRange } from "../../ipc";
import { statementAt } from "../../lib/text";

export type Selection = { from: number; to: number; head: number };

/** `statement`: the selection, else the statement at the cursor. `script`: the selection, else
 * the whole text. A selection always runs every statement in it. */
export type RunMode = "statement" | "script";

export type StatementStatus = "pending" | "running" | "done" | "failed" | "cancelled" | "skipped";

export type StatementRun = {
  sql: string;
  /** UTF-16 offsets in the editor text. */
  start: number;
  end: number;
  /** Why it needs confirmation, from `sql_split`. */
  danger: string | null;
  status: StatementStatus;
  result: QueryResult | null;
  error: AppError | null;
  /** Absolute editor offset of the error, when the server reported a position. */
  errorOffset: number | null;
};

export type RunMessage = { at: number; level: "info" | "error"; text: string };

export type Run = {
  statements: StatementRun[];
  messages: RunMessage[];
  /** The user pressed cancel: the running statement is cancelled and no other starts. */
  cancelRequested: boolean;
};

/** Statements waiting for the user to confirm the dangerous ones among them (task 5.10). */
export type Confirmation = { statements: StatementRun[] };

/** Turns split ranges (relative to `offset` in `text`) into pending statements. */
export function toStatements(text: string, ranges: StatementRange[], offset = 0): StatementRun[] {
  return ranges.map((range) => ({
    sql: text.slice(range.start + offset, range.end + offset),
    start: range.start + offset,
    end: range.end + offset,
    danger: range.danger,
    status: "pending",
    result: null,
    error: null,
    errorOffset: null,
  }));
}

/**
 * The statements a run covers. `ranges` splits the whole text; `selectionRanges` splits the
 * selected text (relative to the selection start) and is only used when there is a selection.
 */
export function plan(
  text: string,
  selection: Selection,
  mode: RunMode,
  ranges: StatementRange[],
  selectionRanges: StatementRange[] = [],
): StatementRun[] {
  if (selection.from !== selection.to) return toStatements(text, selectionRanges, selection.from);
  if (mode === "script") return toStatements(text, ranges);
  const range = statementAt(ranges, selection.head);
  return range ? toStatements(text, [range]) : [];
}

export function positionOf(error: AppError): number | null {
  const detail = error.detail;
  if (typeof detail !== "object" || detail === null) return null;
  const position = (detail as Record<string, unknown>).position;
  return typeof position === "number" ? position : null;
}

/** `3 / 12` while a script runs. */
export function progress(run: Run): { done: number; total: number } {
  const done = run.statements.filter(
    (s) => s.status !== "pending" && s.status !== "running",
  ).length;
  return { done, total: run.statements.length };
}

/** One line for the Messages log. */
export function describeOutcome(index: number, result: QueryResult): string {
  const label = `Statement ${index + 1}`;
  const time = `${result.elapsedMs} ms`;
  if (result.outcome.kind === "affected") {
    const noun = result.outcome.rows === "1" ? "row" : "rows";
    return `${label}: ${result.outcome.rows} ${noun} affected · ${time}`;
  }
  const count = result.outcome.rows.length;
  const more = result.outcome.truncated ? "+" : "";
  return `${label}: ${count}${more} ${count === 1 ? "row" : "rows"} · ${time}`;
}
