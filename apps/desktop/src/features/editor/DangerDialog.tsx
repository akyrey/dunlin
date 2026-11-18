import { Button, Modal } from "@roost/ui";
import type { Confirmation } from "./run";

type Props = {
  confirmation: Confirmation;
  connectionName: string;
  connectionColor: string | null;
  onConfirm: () => void;
  onCancel: () => void;
};

/** Lists every dangerous statement of a run once, before anything runs (task 5.10). */
export function DangerDialog({
  confirmation,
  connectionName,
  connectionColor,
  onConfirm,
  onCancel,
}: Props) {
  const dangerous = confirmation.statements.filter((s) => s.danger !== null);
  const total = confirmation.statements.length;
  const title =
    dangerous.length === 1
      ? "Run a dangerous statement?"
      : `Run ${dangerous.length} dangerous statements?`;
  return (
    <Modal
      title={title}
      onClose={onCancel}
      footer={
        <>
          <Button onClick={onCancel} autoFocus>
            Cancel
          </Button>
          <Button variant="danger" onClick={onConfirm}>
            {total === 1 ? "Run anyway" : `Run all ${total}`}
          </Button>
        </>
      }
    >
      <p className="flex items-center gap-2 text-sm text-zinc-200">
        <span
          aria-hidden="true"
          className="h-3 w-3 shrink-0 rounded-full"
          style={{ backgroundColor: connectionColor ?? "#71717a" }}
        />
        On <strong>{connectionName}</strong>
      </p>
      <ul
        aria-label="Dangerous statements"
        className="mt-3 flex max-h-64 flex-col gap-2 overflow-auto"
      >
        {dangerous.map((statement) => (
          <li key={statement.start}>
            <p className="text-xs text-amber-300">{statement.danger}</p>
            <pre className="mt-1 overflow-auto rounded bg-zinc-950 p-2 font-mono text-xs text-zinc-300">
              {statement.sql}
            </pre>
          </li>
        ))}
      </ul>
    </Modal>
  );
}
