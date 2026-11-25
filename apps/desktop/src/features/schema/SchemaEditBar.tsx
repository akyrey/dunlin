/** The changes bar and preview dialog of a table's staged schema changes (task 9.12). */
import { Button, Modal } from "@roost/ui";

import { LazyReadOnlyCode as ReadOnlyCode } from "../editor/lazy";
import type { SchemaEditing } from "./useSchemaEditing";

type Connection = { name: string; color: string | null };

export function SchemaEditBar({
  editing,
  connection,
}: {
  editing: SchemaEditing;
  connection: Connection;
}) {
  const { count, busy, error } = editing;
  if (count === 0 && !error && !editing.preview) return null;
  return (
    <div className="border-t border-zinc-800">
      {error && <ApplyErrorPanel editing={editing} />}
      {count > 0 && (
        <div
          role="toolbar"
          aria-label="Schema changes"
          className="flex items-center gap-2 px-3 py-1.5 text-xs"
        >
          <span className="text-zinc-300">
            {count} {count === 1 ? "change" : "changes"} not applied
          </span>
          <Button variant="ghost" className="ml-auto text-xs" onClick={editing.discard}>
            Discard
          </Button>
          <Button
            variant="primary"
            className="text-xs"
            disabled={busy}
            onClick={() => {
              void editing.openPreview();
            }}
          >
            {busy && !editing.preview ? "Planning…" : "Preview and apply"}
          </Button>
        </div>
      )}
      {editing.preview && <PreviewDialog editing={editing} connection={connection} />}
    </div>
  );
}

function ApplyErrorPanel({ editing }: { editing: SchemaEditing }) {
  const error = editing.error;
  if (!error) return null;
  return (
    <div
      role="alert"
      className="flex flex-col gap-1 bg-rose-950/40 px-3 py-2 text-xs text-rose-200"
    >
      <div className="flex items-start gap-2">
        <p className="flex-1">{error.message}</p>
        <Button variant="ghost" className="py-0 text-xs" onClick={editing.dismissError}>
          Dismiss
        </Button>
      </div>
      {error.failedStatement && (
        <p>
          Failed: <code className="font-mono">{error.failedStatement}</code>
        </p>
      )}
      {error.applied.length > 0 && (
        <div className="text-amber-200">
          <p>These statements ran before the failure and stayed applied; the table was reloaded:</p>
          <ul className="ml-4 list-disc font-mono">
            {error.applied.map((s) => (
              <li key={s}>{s}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function PreviewDialog({
  editing,
  connection,
}: {
  editing: SchemaEditing;
  connection: Connection;
}) {
  const preview = editing.preview;
  if (!preview) return null;
  return (
    <Modal
      title="Apply schema changes?"
      onClose={editing.cancelPreview}
      footer={
        <>
          <Button onClick={editing.cancelPreview} autoFocus>
            Cancel
          </Button>
          <Button
            variant={preview.destructive ? "danger" : "primary"}
            disabled={editing.busy}
            onClick={() => {
              void editing.apply();
            }}
          >
            {editing.busy ? "Applying…" : preview.destructive ? "Apply and drop" : "Apply"}
          </Button>
        </>
      }
    >
      <p className="flex items-center gap-2 text-sm text-zinc-200">
        <span
          aria-hidden="true"
          className="h-3 w-3 shrink-0 rounded-full"
          style={{ backgroundColor: connection.color ?? "#71717a" }}
        />
        {preview.statements.length} {preview.statements.length === 1 ? "statement" : "statements"}{" "}
        on <strong>{connection.name}</strong>
        {preview.transactional ? ", in one transaction." : ", one at a time."}
      </p>
      {preview.destructive && (
        <p className="mt-2 text-sm text-rose-300">
          These changes drop columns, keys, indexes or constraints, or rebuild the table. Data in a
          dropped column is lost.
        </p>
      )}
      {preview.warnings.length > 0 && (
        <ul aria-label="Warnings" className="mt-2 list-disc pl-5 text-xs text-amber-300">
          {preview.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
      <div className="mt-3 h-64 overflow-hidden rounded border border-zinc-800">
        <ReadOnlyCode
          text={preview.statements.map((s) => `${s};`).join("\n")}
          label="Statements to apply"
          language="sql"
          wrap
        />
      </div>
    </Modal>
  );
}
