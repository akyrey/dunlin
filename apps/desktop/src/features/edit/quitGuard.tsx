import { useEffect, useState } from "react";

import { ConfirmDialog } from "../../components/ConfirmDialog";
import { onQuitRequested, quitApp } from "../../ipc";
import { pendingViews, useEdits } from "./editStore";

/** Asks before the app quits with unapplied edits (task 7.13). */
export function QuitGuard() {
  const [asking, setAsking] = useState(false);

  useEffect(() => {
    let stop: (() => void) | null = null;
    let stopped = false;
    onQuitRequested(() => {
      const pending = pendingViews(useEdits.getState()).length > 0;
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
  return (
    <ConfirmDialog
      title="Quit with unapplied changes?"
      message={`${count} ${count === 1 ? "change has" : "changes have"} not been applied and will be lost. Nothing was written to the database.`}
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
