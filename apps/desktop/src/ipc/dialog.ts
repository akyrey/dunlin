import { open, save } from "@tauri-apps/plugin-dialog";

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

/** Asks for an existing SQLite file. Resolves `null` when cancelled. */
export async function pickDatabaseFile(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: false, filters: SQLITE_FILTERS });
  return typeof selected === "string" ? selected : null;
}

/** Asks where to create a new SQLite file. Resolves `null` when cancelled. */
export async function pickNewDatabaseFile(): Promise<string | null> {
  return save({ filters: SQLITE_FILTERS, defaultPath: "database.db" });
}
