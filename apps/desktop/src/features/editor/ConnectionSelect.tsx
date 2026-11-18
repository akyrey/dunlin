import { useId, useState } from "react";

import { toAppError } from "../../ipc";
import { useConnections } from "../connections/connectionsStore";
import { useSessions } from "../sessions/sessionsStore";
import { useEditor } from "./editorStore";

type Props = {
  tabId: string;
  /** The connection the tab runs on now. */
  connectionId: string;
  disabled: boolean;
  /** Runs the move, after asking about unapplied edits of the result if there are any. */
  guard: (action: () => void) => void;
};

/** Picks the connection an editor tab runs on: opens it if needed and moves the tab there. */
export function ConnectionSelect({ tabId, connectionId, disabled, guard }: Props) {
  const id = useId();
  const profiles = useConnections((s) => s.profiles);
  const [error, setError] = useState<string | null>(null);

  const moveTo = async (target: string) => {
    setError(null);
    try {
      const session = await useSessions.getState().open(target);
      await useEditor.getState().moveTab(tabId, session.sessionId);
    } catch (caught) {
      setError(toAppError(caught).message);
    }
  };

  return (
    <span className="flex items-center gap-1 text-xs text-zinc-400">
      <label htmlFor={id}>Connection</label>
      <select
        id={id}
        className="max-w-40 truncate rounded bg-zinc-800 px-1 py-0.5 text-zinc-200"
        value={connectionId}
        disabled={disabled}
        title={
          disabled
            ? "Finish the running statement or the open transaction to change the connection"
            : "Run this tab on another connection"
        }
        onChange={(e) => {
          const target = e.target.value;
          if (target !== connectionId) guard(() => void moveTo(target));
        }}
      >
        {profiles.map((profile) => (
          <option key={profile.id} value={profile.id}>
            {profile.name}
          </option>
        ))}
      </select>
      {error && (
        <span role="alert" className="text-rose-300">
          {error}
        </span>
      )}
    </span>
  );
}
