import { useId, useState } from "react";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import {
  type AppError,
  type ConnectionDraft,
  type ConnectionSecrets,
  type DriverInfo,
  type OptionField,
  connectionTest,
  toAppError,
} from "../../ipc";
import {
  type PasswordChange,
  defaultTlsMode,
  draftProblems,
  emptyDraft,
  secretsFor,
} from "./draft";
import { Field } from "./fields/Field";
import { FileFields } from "./fields/FileFields";
import { NetworkFields } from "./fields/NetworkFields";
import { PasswordField } from "./fields/PasswordField";
import { TlsFields } from "./fields/TlsFields";

type TestState =
  | { status: "idle" }
  | { status: "testing" }
  | { status: "ok"; message: string }
  | { status: "failed"; error: AppError };

type Props = {
  initial: ConnectionDraft;
  drivers: DriverInfo[];
  /** The saved profile has a password in the keychain. */
  storedPassword: boolean;
  onSave: (draft: ConnectionDraft, secrets?: ConnectionSecrets) => Promise<void>;
  onCancel: () => void;
};

/** The form is driven by `DriverInfo`; it never branches on the driver kind. */
export function ConnectionForm({ initial, drivers, storedPassword, onSave, onCancel }: Props) {
  const driverSelectId = useId();
  const [draft, setDraft] = useState(initial);
  const [password, setPassword] = useState<PasswordChange>({ kind: "keep" });
  const [tlsTouched, setTlsTouched] = useState(initial.id !== null);
  const [test, setTest] = useState<TestState>({ status: "idle" });
  const [saveError, setSaveError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
  const driver = drivers.find((d) => d.kind === draft.driver);
  if (!driver) return null;
  const problems = draftProblems(draft, driver);

  const change = (patch: Partial<ConnectionDraft>) => {
    setDraft((current) => {
      const next = { ...current, ...patch };
      // New connections follow the host: verify-full remotely, prefer on localhost.
      if (!tlsTouched && patch.host !== undefined) {
        return { ...next, tls: { ...next.tls, mode: defaultTlsMode(driver, next.host) } };
      }
      return next;
    });
    setTest({ status: "idle" });
  };

  const switchDriver = (kind: string) => {
    const next = drivers.find((d) => d.kind === kind);
    if (!next) return;
    setDraft({
      ...emptyDraft(next),
      name: draft.name,
      color: draft.color,
      readOnly: draft.readOnly,
    });
    setTlsTouched(false);
    setTest({ status: "idle" });
  };

  const runTest = async () => {
    setTest({ status: "testing" });
    try {
      const result = await connectionTest(draft, secretsFor(password));
      setTest({ status: "ok", message: `${result.serverVersion} · ${result.latencyMs} ms` });
    } catch (error) {
      setTest({ status: "failed", error: toAppError(error) });
    }
  };

  const save = async () => {
    setSaving(true);
    setSaveError(null);
    try {
      await onSave(draft, secretsFor(password));
    } catch (error) {
      setSaveError(toAppError(error));
      setSaving(false);
    }
  };

  const blocked = problems.length > 0;
  return (
    <Modal
      title={draft.id ? `Edit ${initial.name}` : `New ${driver.label} connection`}
      onClose={onCancel}
      footer={
        <>
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button disabled={blocked || test.status === "testing"} onClick={() => void runTest()}>
            {test.status === "testing" ? "Testing…" : "Test connection"}
          </Button>
          <Button variant="primary" disabled={blocked || saving} onClick={() => void save()}>
            Save
          </Button>
        </>
      }
    >
      <div className="flex max-h-[70vh] flex-col gap-3 overflow-y-auto text-sm">
        {draft.id === null && drivers.length > 1 && (
          <Field label="Database type" htmlFor={driverSelectId}>
            <select
              id={driverSelectId}
              className="input"
              value={draft.driver}
              onChange={(e) => {
                switchDriver(e.target.value);
              }}
            >
              {drivers.map((d) => (
                <option key={d.kind} value={d.kind}>
                  {d.label}
                </option>
              ))}
            </select>
          </Field>
        )}
        <Field label="Name">
          <input
            className="input"
            value={draft.name}
            autoFocus
            onChange={(e) => {
              change({ name: e.target.value });
            }}
          />
        </Field>
        {driver.endpoint === "file" ? (
          <FileFields filePath={draft.filePath} options={draft.options} onChange={change} />
        ) : (
          <>
            <NetworkFields draft={draft} onChange={change} />
            <PasswordField
              stored={storedPassword}
              change={password}
              onChange={(next) => {
                setPassword(next);
                setTest({ status: "idle" });
              }}
            />
          </>
        )}
        {driver.tlsModes.length > 0 && (
          <TlsFields
            tls={draft.tls}
            modes={driver.tlsModes}
            onChange={(tls) => {
              setTlsTouched(true);
              change({ tls });
            }}
          />
        )}
        {driver.options.map((option) => (
          <OptionInput
            key={option.key}
            option={option}
            value={draft.options[option.key] ?? option.default ?? ""}
            onChange={(value) => {
              change({ options: { ...draft.options, [option.key]: value } });
            }}
          />
        ))}
        <div className="flex items-center gap-6">
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={draft.readOnly}
              onChange={(e) => {
                change({ readOnly: e.target.checked });
              }}
            />
            Read-only
          </label>
          <label className="flex items-center gap-2">
            Colour
            <input
              type="color"
              aria-label="Colour"
              value={draft.color ?? "#3b82f6"}
              onChange={(e) => {
                change({ color: e.target.value });
              }}
            />
          </label>
        </div>
        <Feedback problems={problems} test={test} saveError={saveError} />
      </div>
    </Modal>
  );
}

function OptionInput({
  option,
  value,
  onChange,
}: {
  option: OptionField;
  value: string;
  onChange: (value: string) => void;
}) {
  if (option.kind === "bool") {
    return (
      <label className="flex items-center gap-2" title={option.help ?? undefined}>
        <input
          type="checkbox"
          checked={value === "true"}
          onChange={(e) => {
            onChange(String(e.target.checked));
          }}
        />
        {option.label}
      </label>
    );
  }
  return (
    <Field label={option.label}>
      <input
        className="input"
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
        }}
      />
    </Field>
  );
}

function Feedback({
  problems,
  test,
  saveError,
}: {
  problems: string[];
  test: TestState;
  saveError: AppError | null;
}) {
  return (
    <div role="status" className="min-h-5 text-xs">
      {problems.length > 0 && <p className="text-zinc-400">{problems.join(" · ")}</p>}
      {test.status === "ok" && <p className="text-emerald-400">Connected: {test.message}</p>}
      {test.status === "failed" && <p className="text-rose-400">{test.error.message}</p>}
      {saveError && <p className="text-rose-400">{saveError.message}</p>}
    </div>
  );
}
