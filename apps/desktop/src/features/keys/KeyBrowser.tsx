import { useEffect, useState } from "react";

import { useWorkspace } from "../workspace/workspaceStore";
import { listOf, useKeys } from "./keysStore";

export type KeySessionInfo = { connectionId: string; sessionId: string; label: string };

const TYPE_STYLE: Record<string, string> = {
  string: "text-emerald-300",
  hash: "text-sky-300",
  list: "text-amber-300",
  set: "text-fuchsia-300",
  zset: "text-rose-300",
  stream: "text-teal-300",
};

function KeyList({ session }: { session: KeySessionInfo }) {
  const { sessionId, label } = session;
  const list = useKeys((s) => listOf(s, sessionId));
  const [pattern, setPattern] = useState(list.pattern);
  useEffect(() => {
    void useKeys.getState().search(sessionId, "*");
  }, [sessionId]);
  return (
    <section aria-label={`Keys of ${label}`} className="flex min-h-0 flex-col">
      <form
        className="flex items-center gap-1 px-3 py-1.5"
        onSubmit={(e) => {
          e.preventDefault();
          void useKeys.getState().search(sessionId, pattern);
        }}
      >
        <input
          className="input min-w-0 flex-1 py-0.5 font-mono text-xs"
          aria-label="Key pattern"
          placeholder="user:*"
          value={pattern}
          onChange={(e) => {
            setPattern(e.target.value);
          }}
        />
        <button type="submit" className="text-xs text-zinc-400 hover:text-zinc-100">
          Scan
        </button>
      </form>
      <p className="px-3 text-[11px] uppercase tracking-wide text-zinc-500">{label}</p>
      {list.error && (
        <p role="alert" className="px-3 py-1 text-xs text-rose-300">
          {list.error.message}
        </p>
      )}
      <ul aria-label="Keys" className="min-h-0 flex-1 overflow-auto">
        {list.keys.map(({ key, keyType }) => (
          <li key={key.id}>
            <button
              type="button"
              className="flex w-full items-center gap-2 px-3 py-0.5 text-left text-xs hover:bg-zinc-800/60"
              title={key.name}
              onClick={() => {
                useWorkspace.getState().openKey(sessionId, key.id, key.name);
              }}
            >
              <span
                className={`w-10 shrink-0 font-mono text-[10px] uppercase ${TYPE_STYLE[keyType] ?? "text-zinc-400"}`}
              >
                {keyType === "ReJSON-RL" ? "json" : keyType}
              </span>
              <span className="truncate font-mono">{key.name}</span>
            </button>
          </li>
        ))}
        {!list.loading && list.keys.length === 0 && !list.error && (
          <li className="px-3 py-1 text-xs text-zinc-500">No keys match {list.pattern}.</li>
        )}
      </ul>
      {(list.cursor !== null || list.loading) && (
        <button
          type="button"
          disabled={list.loading}
          className="px-3 py-1 text-left text-xs text-sky-400 hover:text-sky-300 disabled:text-zinc-500"
          onClick={() => void useKeys.getState().more(sessionId)}
        >
          {list.loading ? "Scanning…" : `Load more (${list.keys.length} so far)`}
        </button>
      )}
    </section>
  );
}

/** The key browser of key-value connections, in place of the schema tree (D-032). */
export function KeyBrowser({ sessions }: { sessions: KeySessionInfo[] }) {
  if (sessions.length === 0) return null;
  return (
    <div className="flex min-h-0 flex-1 flex-col border-t border-zinc-800">
      {sessions.map((session) => (
        <KeyList key={session.sessionId} session={session} />
      ))}
    </div>
  );
}
