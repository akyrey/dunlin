import { useState } from "react";

import { Button, Modal } from "@roost/ui";
import { LazyReadOnlyCode as ReadOnlyCode } from "../editor/lazy";

type Props = {
  statements: string[];
  connectionName: string;
  connectionColor: string | null;
  /** The profile always shows the preview, so it cannot be skipped. */
  alwaysPreview: boolean;
  applying: boolean;
  onApply: (skipNextTime: boolean) => void;
  onCancel: () => void;
};

/** The exact statements Apply will run, in order, in one transaction (task 7.11). */
export function PreviewDialog(props: Props) {
  const {
    statements,
    connectionName,
    connectionColor,
    alwaysPreview,
    applying,
    onApply,
    onCancel,
  } = props;
  const [skip, setSkip] = useState(false);
  const plural = statements.length === 1 ? "statement" : "statements";
  return (
    <Modal
      title="Apply changes?"
      onClose={onCancel}
      footer={
        <>
          {!alwaysPreview && (
            <label className="mr-auto flex items-center gap-2 text-xs text-zinc-400">
              <input
                type="checkbox"
                checked={skip}
                onChange={(e) => {
                  setSkip(e.target.checked);
                }}
              />
              Apply without this preview for the rest of the session
            </label>
          )}
          <Button onClick={onCancel} autoFocus>
            Cancel
          </Button>
          <Button
            variant="primary"
            disabled={applying}
            onClick={() => {
              onApply(skip);
            }}
          >
            {applying ? "Applying…" : "Apply"}
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
        {statements.length} {plural} on <strong>{connectionName}</strong>, in one transaction. Each
        UPDATE and DELETE must change exactly one row, or nothing is applied.
      </p>
      <div className="mt-3 h-64 overflow-hidden rounded border border-zinc-800">
        <ReadOnlyCode
          text={statements.map((s) => `${s};`).join("\n")}
          label="Statements to apply"
          language="sql"
          wrap
        />
      </div>
    </Modal>
  );
}
