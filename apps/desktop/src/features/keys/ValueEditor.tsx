import { Button } from "@roost/ui";
import type { KeyEdit } from "../../ipc";
import { ACTIONS, buildEdit, type Field, fieldsOf, type Inputs } from "./edits";

export type EditRequest = { edit: KeyEdit; removes: boolean };

type Props = {
  keyType: string;
  inputs: Inputs;
  onInputs: (inputs: Inputs) => void;
  onEdit: (request: EditRequest) => void;
  onInvalid: (message: string) => void;
};

const LABELS: Record<Field, string> = {
  value: "Value",
  field: "Field",
  index: "Index",
  member: "Member",
  score: "Score",
};

/** Inputs and actions for the elements of a key; selecting a grid row fills the inputs. */
export function ValueEditor({ keyType, inputs, onInputs, onEdit, onInvalid }: Props) {
  const actions = ACTIONS[keyType];
  if (!actions) return null;
  const multiline = keyType === "string";
  return (
    <form
      aria-label="Edit value"
      className="flex flex-wrap items-end gap-2 border-t border-zinc-800 px-3 py-2 text-xs"
      onSubmit={(e) => {
        e.preventDefault();
      }}
    >
      {fieldsOf(keyType).map((field) => (
        <label
          key={field}
          className={`flex flex-col gap-0.5 text-zinc-400 ${field === "value" ? "min-w-48 flex-1" : "w-32"}`}
        >
          {LABELS[field]}
          {multiline ? (
            <textarea
              className="input h-20 font-mono text-xs"
              value={inputs[field] ?? ""}
              onChange={(e) => {
                onInputs({ ...inputs, [field]: e.target.value });
              }}
            />
          ) : (
            <input
              className="input py-0.5 font-mono text-xs"
              value={inputs[field] ?? ""}
              onChange={(e) => {
                onInputs({ ...inputs, [field]: e.target.value });
              }}
            />
          )}
        </label>
      ))}
      <div className="flex gap-1">
        {actions.map((action) => (
          <Button
            key={action.id}
            variant={action.removes ? "ghost" : "secondary"}
            className="py-0.5 text-xs"
            onClick={() => {
              const edit = buildEdit(action.id, inputs);
              if (typeof edit === "string") onInvalid(edit);
              else onEdit({ edit, removes: action.removes === true });
            }}
          >
            {action.label}
          </Button>
        ))}
      </div>
    </form>
  );
}
