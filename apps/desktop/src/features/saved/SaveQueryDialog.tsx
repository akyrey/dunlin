import { useId, useState } from "react";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { type AppError, toAppError } from "../../ipc";

type Props = {
  initialName: string;
  connectionName: string;
  onSave: (name: string, forEveryConnection: boolean) => Promise<void>;
  onCancel: () => void;
};

/** Names a new saved query and says where it is offered (task 8.7). */
export function SaveQueryDialog({ initialName, connectionName, onSave, onCancel }: Props) {
  const nameId = useId();
  const [name, setName] = useState(initialName);
  const [everywhere, setEverywhere] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const save = async () => {
    try {
      await onSave(name, everywhere);
    } catch (caught) {
      setError(toAppError(caught));
    }
  };
  return (
    <Modal
      title="Save query"
      onClose={onCancel}
      footer={
        <>
          <Button onClick={onCancel}>Cancel</Button>
          <Button variant="primary" disabled={!name.trim()} onClick={() => void save()}>
            Save
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-3 text-sm"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <label htmlFor={nameId} className="flex flex-col gap-1">
          Name
          <input
            id={nameId}
            className="input"
            autoFocus
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
          />
        </label>
        <fieldset className="flex flex-col gap-1">
          <legend className="mb-1 text-xs text-zinc-400">Offer it for</legend>
          <label className="flex items-center gap-2">
            <input
              type="radio"
              checked={!everywhere}
              onChange={() => {
                setEverywhere(false);
              }}
            />
            {connectionName} only
          </label>
          <label className="flex items-center gap-2">
            <input
              type="radio"
              checked={everywhere}
              onChange={() => {
                setEverywhere(true);
              }}
            />
            Every connection
          </label>
        </fieldset>
        {error && (
          <p role="alert" className="text-xs text-rose-300">
            {error.message}
          </p>
        )}
      </form>
    </Modal>
  );
}
