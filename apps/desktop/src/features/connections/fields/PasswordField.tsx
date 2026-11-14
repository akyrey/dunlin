import { useId } from "react";

import type { PasswordChange } from "../draft";

type Props = {
  stored: boolean;
  change: PasswordChange;
  onChange: (change: PasswordChange) => void;
};

/** The stored password is never shown or sent back; the user can only replace or forget it. */
export function PasswordField({ stored, change, onChange }: Props) {
  const id = useId();
  const typed = change.kind === "set" ? change.value : "";
  const placeholder =
    change.kind === "clear" ? "Will be removed" : stored ? "Stored in the keychain" : "";
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-xs text-zinc-400">
        Password
      </label>
      <div className="flex items-center gap-2">
        <input
          id={id}
          type="password"
          className="input flex-1"
          autoComplete="new-password"
          value={typed}
          placeholder={placeholder}
          onChange={(e) => {
            const value = e.target.value;
            onChange(value === "" ? { kind: "keep" } : { kind: "set", value });
          }}
        />
        {stored && (
          <label className="flex items-center gap-1 text-xs text-zinc-400">
            <input
              type="checkbox"
              checked={change.kind === "clear"}
              onChange={(e) => {
                onChange(e.target.checked ? { kind: "clear" } : { kind: "keep" });
              }}
            />
            Forget stored
          </label>
        )}
      </div>
    </div>
  );
}
