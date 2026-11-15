import { useEffect, useId, useState } from "react";

import { Button } from "../../../components/Button";
import {
  type SshAuthMethod,
  type SshHostSettings,
  type SshConfigEntry,
  type SshSettings,
  pickPrivateKeyFile,
  sshConfigHosts,
  toAppError,
} from "../../../ipc";
import { settingsFromConfig } from "../sshConfig";
import {
  DEFAULT_SSH_PORT,
  type PasswordChange,
  type SecretChanges,
  type SecretField,
  emptySsh,
  emptySshHost,
} from "../draft";
import { Field } from "./Field";
import { PasswordField } from "./PasswordField";

const AUTH_LABELS: Record<SshAuthMethod, string> = {
  agent: "SSH agent",
  key_file: "Private key file",
  password: "Password",
};

type Props = {
  ssh: SshSettings | null;
  onChange: (ssh: SshSettings | null) => void;
  secrets: SecretChanges;
  onSecretChange: (field: SecretField, change: PasswordChange) => void;
  /** Secret fields the keychain already holds for this connection. */
  stored: (field: SecretField) => boolean;
};

/** Fills the section from a host alias of `~/.ssh/config` (deferred feature 4). The values
 * are copied: later edits to the file do not change the saved connection. */
function FromSshConfig({ onPick }: { onPick: (ssh: SshSettings) => void }) {
  const id = useId();
  const [entries, setEntries] = useState<SshConfigEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let stale = false;
    sshConfigHosts().then(
      (found) => {
        if (!stale) setEntries(found);
      },
      (caught: unknown) => {
        if (!stale) setError(toAppError(caught).message);
      },
    );
    return () => {
      stale = true;
    };
  }, []);
  if (error) return <p className="text-xs text-amber-400">{error}</p>;
  if (!entries || entries.length === 0) return null;
  return (
    <div className="flex items-center gap-2 text-xs">
      <label htmlFor={id} className="text-zinc-400">
        Fill from ~/.ssh/config
      </label>
      <select
        id={id}
        className="input py-0.5 text-xs"
        value=""
        onChange={(e) => {
          const settings = settingsFromConfig(e.target.value, entries);
          if (settings) onPick(settings);
        }}
      >
        <option value="" disabled>
          Choose a host…
        </option>
        {entries.map((entry) => (
          <option key={entry.alias} value={entry.alias}>
            {entry.alias}
            {entry.hostName && entry.hostName !== entry.alias ? ` (${entry.hostName})` : ""}
          </option>
        ))}
      </select>
    </div>
  );
}

/** The SSH tunnel section: off, one server, or a server behind a jump host. */
export function SshFields({ ssh, onChange, secrets, onSecretChange, stored }: Props) {
  const secretProps = (field: SecretField) => ({
    stored: stored(field),
    change: secrets[field] ?? { kind: "keep" as const },
    onChange: (change: PasswordChange) => {
      onSecretChange(field, change);
    },
  });
  return (
    <fieldset className="flex flex-col gap-2 rounded border border-zinc-800 p-3">
      <legend className="px-1 text-xs text-zinc-400">SSH tunnel</legend>
      <label className="flex items-center gap-2">
        <input
          type="checkbox"
          checked={ssh !== null}
          onChange={(e) => {
            onChange(e.target.checked ? emptySsh() : null);
          }}
        />
        Connect through an SSH server
      </label>
      {ssh && (
        <>
          <FromSshConfig onPick={onChange} />
          <SshHostFields
            title="SSH server"
            value={ssh.server}
            onChange={(server) => {
              onChange({ ...ssh, server });
            }}
            password={secretProps("sshPassword")}
            passphrase={secretProps("sshKeyPassphrase")}
          />
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={ssh.jump !== null}
              onChange={(e) => {
                onChange({ ...ssh, jump: e.target.checked ? emptySshHost() : null });
              }}
            />
            Reach it through a jump host
          </label>
          {ssh.jump && (
            <SshHostFields
              title="Jump host"
              value={ssh.jump}
              onChange={(jump) => {
                onChange({ ...ssh, jump });
              }}
              password={secretProps("jumpPassword")}
              passphrase={secretProps("jumpKeyPassphrase")}
            />
          )}
        </>
      )}
    </fieldset>
  );
}

type SecretFieldProps = {
  stored: boolean;
  change: PasswordChange;
  onChange: (c: PasswordChange) => void;
};

function SshHostFields({
  title,
  value,
  onChange,
  password,
  passphrase,
}: {
  title: string;
  value: SshHostSettings;
  onChange: (value: SshHostSettings) => void;
  password: SecretFieldProps;
  passphrase: SecretFieldProps;
}) {
  const authId = useId();
  const update = (patch: Partial<SshHostSettings>) => {
    onChange({ ...value, ...patch });
  };
  const chooseKey = async () => {
    const path = await pickPrivateKeyFile();
    if (path) update({ keyFile: path });
  };
  return (
    <div
      role="group"
      aria-label={title}
      className="flex flex-col gap-2 border-l-2 border-zinc-800 pl-3"
    >
      <div className="flex gap-2">
        <div className="flex-1">
          <Field label={`${title} host`}>
            <input
              className="input w-full"
              value={value.host}
              onChange={(e) => {
                update({ host: e.target.value });
              }}
            />
          </Field>
        </div>
        <div className="w-20">
          <Field label={`${title} port`}>
            <input
              className="input w-full"
              inputMode="numeric"
              value={value.port}
              onChange={(e) => {
                update({ port: Number.parseInt(e.target.value, 10) || DEFAULT_SSH_PORT });
              }}
            />
          </Field>
        </div>
      </div>
      <Field label={`${title} user`}>
        <input
          className="input"
          value={value.user}
          onChange={(e) => {
            update({ user: e.target.value });
          }}
        />
      </Field>
      <Field label={`${title} authentication`} htmlFor={authId}>
        <select
          id={authId}
          className="input"
          value={value.auth}
          onChange={(e) => {
            update({ auth: e.target.value as SshAuthMethod });
          }}
        >
          {(Object.keys(AUTH_LABELS) as SshAuthMethod[]).map((auth) => (
            <option key={auth} value={auth}>
              {AUTH_LABELS[auth]}
            </option>
          ))}
        </select>
      </Field>
      {value.auth === "key_file" && (
        <>
          <Field label={`${title} key file`}>
            <div className="flex gap-2">
              <input
                className="input flex-1 font-mono text-xs"
                value={value.keyFile ?? ""}
                placeholder="~/.ssh/id_ed25519"
                onChange={(e) => {
                  update({ keyFile: e.target.value.trim() === "" ? null : e.target.value });
                }}
              />
              <Button onClick={() => void chooseKey()}>Browse…</Button>
            </div>
          </Field>
          <PasswordField label={`${title} key passphrase`} {...passphrase} />
        </>
      )}
      {value.auth === "password" && <PasswordField label={`${title} password`} {...password} />}
    </div>
  );
}
