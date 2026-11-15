/** The one place the frontend maps a driver to its editor language (task 5.3, NFR-6). A new
 * driver registers its dialect here and nowhere else. */
import { MySQL, PostgreSQL, type SQLDialect, SQLite } from "@codemirror/lang-sql";

import type { DriverKind } from "../../ipc";

/** `null`: not a SQL database; the editor uses that driver's own command language. */
const DIALECTS: Record<DriverKind, SQLDialect | null> = {
  postgres: PostgreSQL,
  mysql: MySQL,
  sqlite: SQLite,
  redis: null,
};

export function dialectFor(driver: DriverKind): SQLDialect | null {
  return DIALECTS[driver];
}
