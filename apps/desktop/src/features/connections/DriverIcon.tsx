import { type SimpleIcon, siMysql, siPostgresql, siRedis, siSqlite } from "simple-icons";

import type { DriverKind } from "../../ipc";

/**
 * Logo and accessible name per driver. Presentation only: features still come from
 * `Capabilities`, never from the driver kind (D-041). A `Record` makes a new driver
 * a compile error here until it has an icon.
 */
const ICONS: Record<DriverKind, { icon: SimpleIcon; label: string }> = {
  postgres: { icon: siPostgresql, label: "PostgreSQL" },
  mysql: { icon: siMysql, label: "MySQL / MariaDB" },
  sqlite: { icon: siSqlite, label: "SQLite" },
  redis: { icon: siRedis, label: "Redis / Valkey" },
};

type Props = { driver: DriverKind; className?: string };

/** The database's logo, drawn in the current text colour. */
export function DriverIcon({ driver, className = "h-3.5 w-3.5" }: Props) {
  const { icon, label } = ICONS[driver];
  return (
    <svg
      role="img"
      aria-label={label}
      viewBox="0 0 24 24"
      fill="currentColor"
      className={`shrink-0 ${className}`}
    >
      <title>{label}</title>
      <path d={icon.path} />
    </svg>
  );
}
