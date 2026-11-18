import { useEffect, useState } from "react";

import { ConfirmDialog } from "@roost/ui";
import { onQuitRequested, quitApp } from "../../ipc";
import { describeUnsavedWork, hasUnsavedWork, unsavedWork } from "./unsavedWork";

/** Asks before the app quits with unapplied edits (task 7.13). */
export function QuitGuard() {
  const [asking, setAsking] = useState(false);

  useEffect(() => {
    let stop: (() => void) | null = null;
    let stopped = false;
    onQuitRequested(() => {
      const pending = hasUnsavedWork(unsavedWork());
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
  return (
    <ConfirmDialog
      title="Quit with unapplied changes?"
      message={`${describeUnsavedWork(unsavedWork())} Nothing uncommitted is written to the database.`}
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
