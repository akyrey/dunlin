/** The one place the frontend maps a driver to its SQL dialect (task 5.3, NFR-6). A new driver
 * registers its dialect here and nowhere else. */
import { MySQL, PostgreSQL, type SQLDialect, SQLite } from "@codemirror/lang-sql";

import type { DriverKind } from "../../ipc";

const DIALECTS: Record<DriverKind, SQLDialect> = {
  postgres: PostgreSQL,
  mysql: MySQL,
  sqlite: SQLite,
};

export function dialectFor(driver: DriverKind): SQLDialect {
  return DIALECTS[driver];
}
