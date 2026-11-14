import type { ConnectionProfile } from "../../ipc";

export type FolderGroup = { folder: string | null; profiles: ConnectionProfile[] };

/** Connections without a folder first, then each folder by name; the list order inside each. */
export function groupByFolder(profiles: readonly ConnectionProfile[]): FolderGroup[] {
  const folders = [...new Set(profiles.flatMap((p) => (p.folder ? [p.folder] : [])))].sort((a, b) =>
    a.localeCompare(b, undefined, { sensitivity: "base" }),
  );
  const loose = profiles.filter((p) => !p.folder);
  return [
    ...(loose.length > 0 ? [{ folder: null, profiles: loose }] : []),
    ...folders.map((folder) => ({ folder, profiles: profiles.filter((p) => p.folder === folder) })),
  ];
}
