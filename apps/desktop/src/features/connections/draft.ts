import type { ConnectionDraft, ConnectionProfile, DriverInfo } from "../../ipc";

export const DEFAULT_COLOR = "#3b82f6";

/** A new, empty profile for `driver`, with its options at their declared defaults. */
export function emptyDraft(driver: DriverInfo): ConnectionDraft {
  const options: Record<string, string> = {};
  for (const option of driver.options) {
    if (option.default !== null) options[option.key] = option.default;
  }
  return {
    id: null,
    name: "",
    driver: driver.kind,
    color: DEFAULT_COLOR,
    readOnly: false,
    host: null,
    port: null,
    database: null,
    user: null,
    filePath: null,
    options,
  };
}

export function draftFromProfile(profile: ConnectionProfile): ConnectionDraft {
  return { ...profile, options: { ...profile.options } };
}

/** Client-side checks that mirror the backend's, for immediate feedback. */
export function draftProblems(draft: ConnectionDraft, driver: DriverInfo): string[] {
  const problems: string[] = [];
  if (draft.name.trim() === "") problems.push("Enter a name");
  if (driver.endpoint === "file" && !draft.filePath?.trim())
    problems.push("Choose a database file");
  if (driver.endpoint === "tcp" && !draft.host?.trim()) problems.push("Enter a host");
  return problems;
}
