import { useState } from "react";

import { Button, Modal } from "@roost/ui";
import { isAppError, type KeyEdit, kvRead } from "../../ipc";
import { ACTIONS, buildEdit, CREATE_ACTIONS, type Inputs, keyIdOf } from "./edits";
import { KeyEditDialog } from "./KeyEditDialog";

type Props = {
  sessionId: string;
  onCreated: (id: string, name: string) => void;
  onClose: () => void;
};

const TYPES = Object.keys(CREATE_ACTIONS);

/** A new key with its first element; the commands are previewed before they run (D-032). */
export function NewKeyDialog({ sessionId, onCreated, onClose }: Props) {
  const [name, setName] = useState("");
  const [keyType, setKeyType] = useState("string");
  const [inputs, setInputs] = useState<Inputs>({});
  const [problem, setProblem] = useState<string | null>(null);
  const [edit, setEdit] = useState<KeyEdit | null>(null);
  const action = ACTIONS[keyType]?.find((a) => a.id === CREATE_ACTIONS[keyType]);

  const review = async () => {
    if (!action) return;
    if (name === "") {
      setProblem("The key needs a name.");
      return;
    }
    const built = buildEdit(action.id, inputs);
    if (typeof built === "string") {
      setProblem(built);
      return;
    }
    // Refuse an existing key rather than overwrite or extend it from a "new key" form.
    const exists = await kvRead(sessionId, keyIdOf(name), 1).then(
      () => true,
      (caught: unknown) => !(isAppError(caught) && caught.code === "not_found"),
    );
    if (exists) {
      setProblem("A key with this name already exists, or it could not be checked.");
      return;
    }
    setProblem(null);
    setEdit(built);
  };

  if (edit) {
    return (
      <KeyEditDialog
        sessionId={sessionId}
        keyId={keyIdOf(name)}
        edit={edit}
        onClose={() => {
          setEdit(null);
        }}
        onApplied={() => {
          onCreated(keyIdOf(name), name);
        }}
      />
    );
  }

  return (
    <Modal
      title="New key"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" form="new-key">
            Review
          </Button>
        </>
      }
    >
      <form
        id="new-key"
        className="flex flex-col gap-3 text-sm"
        onSubmit={(e) => {
          e.preventDefault();
          void review();
        }}
      >
        <label className="flex flex-col gap-1">
          Key name
          <input
            className="input font-mono"
            value={name}
            autoFocus
            onChange={(e) => {
              setName(e.target.value);
            }}
          />
        </label>
        <label className="flex flex-col gap-1">
          Type
          <select
            className="input"
            value={keyType}
            onChange={(e) => {
              setKeyType(e.target.value);
            }}
          >
            {TYPES.map((type) => (
              <option key={type} value={type}>
                {type}
              </option>
            ))}
          </select>
        </label>
        {action?.fields.map((field) => (
          <label key={field} className="flex flex-col gap-1 capitalize">
            {field}
            <input
              className="input font-mono"
              value={inputs[field] ?? ""}
              onChange={(e) => {
                setInputs({ ...inputs, [field]: e.target.value });
              }}
            />
          </label>
        ))}
        {problem && (
          <p role="alert" className="text-rose-300">
            {problem}
          </p>
        )}
      </form>
    </Modal>
  );
}
