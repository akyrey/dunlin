import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

const SQLITE_FILTERS = [
  { name: "SQLite database", extensions: ["db", "sqlite", "sqlite3", "db3"] },
  { name: "All files", extensions: ["*"] },
];

const PEM_FILTERS = [
  { name: "PEM certificate or key", extensions: ["pem", "crt", "cer", "key"] },
  { name: "All files", extensions: ["*"] },
];

/** Asks for a certificate or key file. Resolves `null` when cancelled. */
export async function pickPemFile(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: false, filters: PEM_FILTERS });
  return typeof selected === "string" ? selected : null;
}

/** Asks for an SSH private key. Resolves `null` when cancelled. */
export async function pickPrivateKeyFile(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: false });
  return typeof selected === "string" ? selected : null;
}

/** Asks for an existing SQLite file. Resolves `null` when cancelled. */
export async function pickDatabaseFile(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: false, filters: SQLITE_FILTERS });
  return typeof selected === "string" ? selected : null;
}

/** Asks where to create a new SQLite file. Resolves `null` when cancelled. */
export async function pickNewDatabaseFile(): Promise<string | null> {
  return save({ filters: SQLITE_FILTERS, defaultPath: "database.db" });
}

export type ExportKind = "csv" | "json" | "insert";

const EXPORT_FILTERS: Record<ExportKind, { name: string; extensions: string[] }> = {
  csv: { name: "CSV", extensions: ["csv"] },
  json: { name: "JSON", extensions: ["json"] },
  insert: { name: "SQL", extensions: ["sql"] },
};

/** Asks where to save an export. Resolves `null` when cancelled. */
export async function pickExportFile(
  kind: ExportKind,
  defaultName: string,
): Promise<string | null> {
  const filter = EXPORT_FILTERS[kind];
  return save({ filters: [filter], defaultPath: `${defaultName}.${filter.extensions[0] ?? kind}` });
}

const CONNECTIONS_FILTERS = [{ name: "Dunlin connections", extensions: ["json"] }];

/** Asks where to save exported connections. Resolves `null` when cancelled. */
export async function pickConnectionsExportFile(): Promise<string | null> {
  return save({ filters: CONNECTIONS_FILTERS, defaultPath: "dunlin-connections.json" });
}

/** Asks for a connections file to import. Resolves `null` when cancelled. */
export async function pickConnectionsFile(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: false, filters: CONNECTIONS_FILTERS });
  return typeof selected === "string" ? selected : null;
}

const CSV_FILTERS = [
  { name: "CSV", extensions: ["csv", "tsv", "txt"] },
  { name: "All files", extensions: ["*"] },
];

/** Asks for a CSV file to import. Resolves `null` when cancelled. */
export async function pickCsvFile(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: false, filters: CSV_FILTERS });
  return typeof selected === "string" ? selected : null;
}

/** Shows a file in Finder (or the platform's file manager). */
export function revealFile(path: string): Promise<void> {
  return revealItemInDir(path);
}
