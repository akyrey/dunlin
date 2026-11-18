import { useId } from "react";

import { Button } from "@roost/ui";
import { type TlsMode, type TlsSettings, pickPemFile } from "../../../ipc";
import { isUnverified } from "../draft";
import { Field } from "./Field";

const MODE_LABELS: Record<TlsMode, string> = {
  disable: "Disable",
  prefer: "Prefer (unverified)",
  require: "Require (unverified)",
  "verify-ca": "Verify CA",
  "verify-full": "Verify CA and host name",
};

type Props = {
  tls: TlsSettings;
  modes: TlsMode[];
  onChange: (tls: TlsSettings) => void;
};

export function TlsFields({ tls, modes, onChange }: Props) {
  const modeId = useId();
  const update = (patch: Partial<TlsSettings>) => {
    onChange({ ...tls, ...patch });
  };
  return (
    <fieldset className="flex flex-col gap-2 rounded border border-zinc-800 p-3">
      <legend className="px-1 text-xs text-zinc-400">TLS</legend>
      <Field label="Mode" htmlFor={modeId}>
        <select
          id={modeId}
          className="input w-full"
          value={tls.mode}
          onChange={(e) => {
            update({ mode: e.target.value as TlsMode });
          }}
        >
          {modes.map((mode) => (
            <option key={mode} value={mode}>
              {MODE_LABELS[mode]}
            </option>
          ))}
        </select>
      </Field>
      {isUnverified(tls.mode) && (
        <p role="note" className="rounded bg-amber-950/60 px-2 py-1 text-xs text-amber-300">
          The connection is encrypted, but the server certificate is not checked, so an attacker on
          the network could impersonate the server.
        </p>
      )}
      {tls.mode !== "disable" && (
        <>
          <PemField
            label="CA certificate"
            value={tls.caFile}
            onChange={(caFile) => {
              update({ caFile });
            }}
          />
          <PemField
            label="Client certificate"
            value={tls.certFile}
            onChange={(certFile) => {
              update({ certFile });
            }}
          />
          <PemField
            label="Client key"
            value={tls.keyFile}
            onChange={(keyFile) => {
              update({ keyFile });
            }}
          />
        </>
      )}
    </fieldset>
  );
}

function PemField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string | null;
  onChange: (path: string | null) => void;
}) {
  const choose = async () => {
    const path = await pickPemFile();
    if (path) onChange(path);
  };
  return (
    <Field label={label}>
      <div className="flex gap-2">
        <input
          className="input flex-1 font-mono text-xs"
          value={value ?? ""}
          placeholder="Optional"
          onChange={(e) => {
            onChange(e.target.value.trim() === "" ? null : e.target.value);
          }}
        />
        <Button onClick={() => void choose()}>Browse…</Button>
      </div>
    </Field>
  );
}
