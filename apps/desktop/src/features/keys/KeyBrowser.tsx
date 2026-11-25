import { useEffect, useState } from "react";

import { toast } from "@roost/ui";
import type { DriverKind } from "../../ipc";
import { ConnectionMark } from "../connections/ConnectionMark";
import { useWorkspace } from "../workspace/workspaceStore";
import { KeyTreeList } from "./KeyTreeList";
import { listOf, useKeys } from "./keysStore";
import { NewKeyDialog } from "./NewKeyDialog";

export type KeySessionInfo = {
  connectionId: string;
  sessionId: string;
  label: string;
  driver: DriverKind;
  color: string | null;
  readOnly?: boolean;
};

function KeyList({ session }: { session: KeySessionInfo }) {
  const { sessionId, label } = session;
  const list = useKeys((s) => listOf(s, sessionId));
  const [pattern, setPattern] = useState(list.pattern);
  const [creating, setCreating] = useState(false);
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
      <div className="flex items-center gap-1.5 px-3">
        <ConnectionMark driver={session.driver} color={session.color} />
        <p className="flex-1 truncate text-[11px] uppercase tracking-wide text-zinc-500">{label}</p>
        {!session.readOnly && (
          <button
            type="button"
            className="text-xs text-zinc-400 hover:text-zinc-100"
            onClick={() => {
              setCreating(true);
            }}
          >
            New key
          </button>
        )}
      </div>
      {creating && (
        <NewKeyDialog
          sessionId={sessionId}
          onClose={() => {
            setCreating(false);
          }}
          onCreated={(id, name) => {
            setCreating(false);
            toast.success(`Created ${name}`);
            void useKeys.getState().search(sessionId, list.pattern);
            useWorkspace.getState().openKey(sessionId, id, name);
          }}
        />
      )}
      {list.error && (
        <p role="alert" className="px-3 py-1 text-xs text-rose-300">
          {list.error.message}
        </p>
      )}
      <ul aria-label="Keys" className="min-h-0 flex-1 overflow-auto">
        <KeyTreeList
          keys={list.keys}
          onOpen={({ key }) => {
            useWorkspace.getState().openKey(sessionId, key.id, key.name);
          }}
        />
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
