import { useEffect, useState } from "react";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { type AppError, type KeyEdit, kvEdit, toAppError } from "../../ipc";

type Props = {
  sessionId: string;
  /** The key's base64 id. */
  keyId: string;
  edit: KeyEdit;
  /** The change removes data: Apply is styled as dangerous. */
  removes?: boolean;
  onApplied: () => void;
  onClose: () => void;
};

/** Shows the exact commands an edit runs; Apply runs them (D-032). */
export function KeyEditDialog({ sessionId, keyId, edit, removes, onApplied, onClose }: Props) {
  const [commands, setCommands] = useState<string[] | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    let stale = false;
    kvEdit(sessionId, keyId, edit, true).then(
      (planned) => {
        if (!stale) setCommands(planned);
      },
      (caught: unknown) => {
        if (!stale) setError(toAppError(caught));
      },
    );
    return () => {
      stale = true;
    };
  }, [sessionId, keyId, edit]);

  const apply = async () => {
    setRunning(true);
    setError(null);
    try {
      await kvEdit(sessionId, keyId, edit, false);
      onApplied();
    } catch (caught: unknown) {
      setError(toAppError(caught));
      setRunning(false);
    }
  };

  return (
    <Modal
      title="Run these commands?"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant={removes ? "danger" : "primary"}
            disabled={commands === null || running}
            onClick={() => void apply()}
          >
            {running ? "Applying…" : "Apply"}
          </Button>
        </>
      }
    >
      {commands === null && !error && <p className="text-sm text-zinc-400">Preparing…</p>}
      {commands && (
        <pre
          aria-label="Commands"
          className="max-h-64 overflow-auto rounded bg-zinc-950 p-2 font-mono text-xs text-zinc-200"
        >
          {commands.join("\n")}
        </pre>
      )}
      {error && (
        <p role="alert" className="mt-2 text-sm text-rose-300">
          {error.message}
        </p>
      )}
    </Modal>
  );
}
