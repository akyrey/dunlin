import type { ConnectionDraft } from "../../../ipc";
import { Field } from "./Field";

const MAX_PORT = 65_535;

type Props = {
  draft: ConnectionDraft;
  onChange: (patch: Partial<ConnectionDraft>) => void;
};

/** Empty input means "not set", so the backend can fall back to defaults. */
function orNull(value: string): string | null {
  return value.trim() === "" ? null : value;
}

function parsePort(value: string): number | null {
  const port = Number.parseInt(value, 10);
  return Number.isInteger(port) && port > 0 && port <= MAX_PORT ? port : null;
}

export function NetworkFields({ draft, onChange }: Props) {
  return (
    <>
      <div className="flex gap-2">
        <div className="flex-1">
          <Field label="Host">
            <input
              className="input w-full"
              value={draft.host ?? ""}
              onChange={(e) => {
                onChange({ host: orNull(e.target.value) });
              }}
            />
          </Field>
        </div>
        <div className="w-24">
          <Field label="Port">
            <input
              className="input w-full"
              inputMode="numeric"
              value={draft.port ?? ""}
              onChange={(e) => {
                onChange({ port: parsePort(e.target.value) });
              }}
            />
          </Field>
        </div>
      </div>
      <div className="flex gap-2">
        <div className="flex-1">
          <Field label="Database">
            <input
              className="input w-full"
              value={draft.database ?? ""}
              onChange={(e) => {
                onChange({ database: orNull(e.target.value) });
              }}
            />
          </Field>
        </div>
        <div className="flex-1">
          <Field label="User">
            <input
              className="input w-full"
              autoComplete="off"
              value={draft.user ?? ""}
              onChange={(e) => {
                onChange({ user: orNull(e.target.value) });
              }}
            />
          </Field>
        </div>
      </div>
    </>
  );
}
