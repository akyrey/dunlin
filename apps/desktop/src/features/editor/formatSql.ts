/** Pretty-prints SQL in the editor's dialect. Only whitespace changes: keyword and identifier
 * case stay as written. */
import { type DialectOptions, formatDialect, mysql, postgresql, sqlite } from "sql-formatter";

import type { DriverKind } from "../../ipc";

export type FormatResult = { ok: true; text: string } | { ok: false; error: string };

const INDENT_WIDTH = 2;

/** `null`: not SQL, nothing to format. */
const FORMATTER_DIALECTS: Record<DriverKind, DialectOptions | null> = {
  postgres: postgresql,
  mysql,
  sqlite,
  redis: null,
};

/** A `DELIMITER` line changes what ends a statement, which the formatter does not know. */
const DELIMITER_LINE = /^\s*delimiter\s/im;

export function formatSql(text: string, driver: DriverKind): FormatResult {
  const dialect = FORMATTER_DIALECTS[driver];
  if (!dialect) return { ok: false, error: "Only SQL can be formatted" };
  if (DELIMITER_LINE.test(text)) {
    return { ok: false, error: "Scripts with DELIMITER can't be formatted" };
  }
  try {
    return {
      ok: true,
      text: formatDialect(text, { dialect, tabWidth: INDENT_WIDTH, keywordCase: "preserve" }),
    };
  } catch (caught) {
    const message = caught instanceof Error ? caught.message : String(caught);
    return { ok: false, error: message.split("\n")[0] ?? message };
  }
}
