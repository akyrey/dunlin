import { useEffect, useState } from "react";

import { ConfirmDialog } from "../../components/ConfirmDialog";
import { onQuitRequested, quitApp } from "../../ipc";
import { useEditor } from "../editor/editorStore";
import { pendingViews, useEdits } from "./editStore";

/** Asks before the app quits with unapplied edits (task 7.13). */
export function QuitGuard() {
  const [asking, setAsking] = useState(false);

  useEffect(() => {
    let stop: (() => void) | null = null;
    let stopped = false;
    onQuitRequested(() => {
      const pending =
        pendingViews(useEdits.getState()).length > 0 ||
        Object.values(useEditor.getState().tabs).some((t) => t.transaction.open);
      if (pending) setAsking(true);
      return pending;
    }).then(
      (unlisten) => {
        if (stopped) unlisten();
        else stop = unlisten;
      },
      () => {
        // Without the listener the window closes as usual; edits are only staged, never written.
      },
    );
    return () => {
      stopped = true;
      stop?.();
    };
  }, []);
  if (!asking) return null;
  const count = Object.values(useEdits.getState().pending).reduce((sum, v) => sum + v.count, 0);
  const open = Object.values(useEditor.getState().tabs).filter((t) => t.transaction.open).length;
  const parts = [
    count > 0 &&
      `${count} ${count === 1 ? "change has" : "changes have"} not been applied and will be lost.`,
    open > 0 &&
      `${open} ${open === 1 ? "tab has an open transaction" : "tabs have open transactions"}, which will be rolled back.`,
  ].filter(Boolean);
  return (
    <ConfirmDialog
      title="Quit with unapplied changes?"
      message={`${parts.join(" ")} Nothing uncommitted is written to the database.`}
      confirmLabel="Quit and discard"
      onConfirm={() => {
        void quitApp();
      }}
      onCancel={() => {
        setAsking(false);
      }}
    />
  );
}
