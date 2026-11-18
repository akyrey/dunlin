import { useState } from "react";

import { Button } from "@roost/ui";
import type { KeyEdit } from "../../ipc";
import { ttlEdit } from "./edits";

export type KeyAction =
  | { kind: "ttl"; edit: KeyEdit }
  | { kind: "rename"; edit: KeyEdit; newName: string }
  | { kind: "delete"; edit: KeyEdit };

type Props = {
  name: string;
  onAction: (action: KeyAction) => void;
  onInvalid: (message: string) => void;
};

/** TTL, rename and delete for the whole key. */
export function KeyActions({ name, onAction, onInvalid }: Props) {
  const [ttl, setTtl] = useState("");
  const [newName, setNewName] = useState(name);
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-zinc-800 px-3 py-1.5 text-xs">
      <form
        className="flex items-center gap-1"
        onSubmit={(e) => {
          e.preventDefault();
          const edit = ttlEdit(ttl);
          if (typeof edit === "string") onInvalid(edit);
          else onAction({ kind: "ttl", edit });
        }}
      >
        <input
          className="input w-24 py-0.5 font-mono text-xs"
          aria-label="TTL in seconds"
          placeholder="seconds"
          inputMode="decimal"
          value={ttl}
          onChange={(e) => {
            setTtl(e.target.value);
          }}
        />
        <Button type="submit" variant="ghost" className="py-0.5 text-xs">
          {ttl.trim() === "" ? "Persist" : "Set TTL"}
        </Button>
      </form>
      <form
        className="flex items-center gap-1"
        onSubmit={(e) => {
          e.preventDefault();
          if (newName === "") onInvalid("The new name cannot be empty.");
          else if (newName === name) onInvalid("The new name is the current name.");
          else onAction({ kind: "rename", edit: { kind: "rename", new_name: newName }, newName });
        }}
      >
        <input
          className="input w-48 py-0.5 font-mono text-xs"
          aria-label="New key name"
          value={newName}
          onChange={(e) => {
            setNewName(e.target.value);
          }}
        />
        <Button type="submit" variant="ghost" className="py-0.5 text-xs">
          Rename
        </Button>
      </form>
      <Button
        variant="ghost"
        className="ml-auto py-0.5 text-xs text-rose-300"
        onClick={() => {
          onAction({ kind: "delete", edit: { kind: "delete" } });
        }}
      >
        Delete key
      </Button>
    </div>
  );
}
