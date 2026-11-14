import type { ConnectionProfile, QueryResult, SessionInfo } from "../ipc";

type Props = {
  profile: ConnectionProfile | null;
  session: SessionInfo | null;
  result: QueryResult | null;
};

export function StatusBar({ profile, session, result }: Props) {
  return (
    <footer className="flex h-7 items-center gap-3 border-t border-zinc-800 bg-zinc-900 px-3 text-xs text-zinc-400">
      {profile ? (
        <span className="flex items-center gap-2">
          <span
            aria-hidden="true"
            className="h-2.5 w-2.5 rounded-full"
            style={{ backgroundColor: profile.color ?? "transparent" }}
          />
          <span className="text-zinc-200">{profile.name}</span>
          {session?.readOnly && (
            <span className="rounded bg-amber-900/60 px-1.5 uppercase text-amber-300">
              read-only
            </span>
          )}
        </span>
      ) : (
        <span>No connection open</span>
      )}
      <span className="flex-1" />
      {result && <span>{summary(result)}</span>}
    </footer>
  );
}

function summary(result: QueryResult): string {
  const time = `${result.elapsedMs} ms`;
  if (result.outcome.kind === "affected") return `${result.outcome.rows} affected · ${time}`;
  const count = result.outcome.rows.length;
  const more = result.outcome.truncated ? "+" : "";
  return `${count}${more} ${count === 1 ? "row" : "rows"} · ${time}`;
}
