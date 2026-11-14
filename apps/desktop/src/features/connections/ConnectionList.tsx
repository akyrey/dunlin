import type { ConnectionProfile } from "../../ipc";

type Props = {
  profiles: ConnectionProfile[];
  openIds: Set<string>;
  activeId: string | null;
  onOpen: (profile: ConnectionProfile) => void;
  onEdit: (profile: ConnectionProfile) => void;
  onDelete: (profile: ConnectionProfile) => void;
};

export function ConnectionList({ profiles, openIds, activeId, onOpen, onEdit, onDelete }: Props) {
  if (profiles.length === 0) {
    return <p className="px-3 py-2 text-xs text-zinc-500">No connections yet.</p>;
  }
  return (
    <ul aria-label="Connections" className="flex flex-col">
      {profiles.map((profile) => (
        <li
          key={profile.id}
          className={`group flex items-center gap-2 px-3 py-1.5 text-sm ${
            profile.id === activeId ? "bg-zinc-800" : "hover:bg-zinc-800/60"
          }`}
        >
          <span
            aria-hidden="true"
            className="h-2.5 w-2.5 shrink-0 rounded-full"
            style={{ backgroundColor: profile.color ?? "transparent" }}
          />
          <button
            type="button"
            className="flex-1 truncate text-left"
            onClick={() => {
              onOpen(profile);
            }}
            title={profile.filePath ?? profile.host ?? ""}
          >
            {profile.name}
            {openIds.has(profile.id) && <span className="sr-only"> (open)</span>}
          </button>
          {openIds.has(profile.id) && (
            <span aria-hidden="true" className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
          )}
          {profile.readOnly && (
            <span className="rounded bg-amber-900/60 px-1 text-[10px] uppercase text-amber-300">
              ro
            </span>
          )}
          <span className="hidden gap-1 group-hover:flex group-focus-within:flex">
            <button
              type="button"
              className="text-xs text-zinc-400 hover:text-zinc-100"
              aria-label={`Edit ${profile.name}`}
              onClick={() => {
                onEdit(profile);
              }}
            >
              Edit
            </button>
            <button
              type="button"
              className="text-xs text-zinc-400 hover:text-rose-300"
              aria-label={`Delete ${profile.name}`}
              onClick={() => {
                onDelete(profile);
              }}
            >
              Delete
            </button>
          </span>
        </li>
      ))}
    </ul>
  );
}
