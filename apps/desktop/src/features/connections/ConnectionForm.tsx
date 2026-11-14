import { useState } from "react";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import {
  type AppError,
  type ConnectionDraft,
  type DriverInfo,
  type OptionField,
  connectionTest,
  pickDatabaseFile,
  pickNewDatabaseFile,
  toAppError,
} from "../../ipc";
import { draftProblems } from "./draft";

const CREATE_OPTION = "create_if_missing";

type TestState =
  | { status: "idle" }
  | { status: "testing" }
  | { status: "ok"; message: string }
  | { status: "failed"; error: AppError };

type Props = {
  initial: ConnectionDraft;
  driver: DriverInfo;
  onSave: (draft: ConnectionDraft) => Promise<void>;
  onCancel: () => void;
};

export function ConnectionForm({ initial, driver, onSave, onCancel }: Props) {
  const [draft, setDraft] = useState(initial);
  const [test, setTest] = useState<TestState>({ status: "idle" });
  const [saveError, setSaveError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
  const problems = draftProblems(draft, driver);

  const change = (patch: Partial<ConnectionDraft>) => {
    setDraft((current) => ({ ...current, ...patch }));
    setTest({ status: "idle" });
  };
  const setOption = (key: string, value: string) => {
    change({ options: { ...draft.options, [key]: value } });
  };

  const runTest = async () => {
    setTest({ status: "testing" });
    try {
      const result = await connectionTest(draft);
      setTest({ status: "ok", message: `${result.serverVersion} · ${result.latencyMs} ms` });
    } catch (error) {
      setTest({ status: "failed", error: toAppError(error) });
    }
  };

  const save = async () => {
    setSaving(true);
    setSaveError(null);
    try {
      await onSave(draft);
    } catch (error) {
      setSaveError(toAppError(error));
      setSaving(false);
    }
  };

  const chooseExisting = async () => {
    const path = await pickDatabaseFile();
    if (path) change({ filePath: path, options: { ...draft.options, [CREATE_OPTION]: "false" } });
  };
  const chooseNew = async () => {
    const path = await pickNewDatabaseFile();
    if (path) change({ filePath: path, options: { ...draft.options, [CREATE_OPTION]: "true" } });
  };

  return (
    <Modal
      title={draft.id ? `Edit ${initial.name}` : `New ${driver.label} connection`}
      onClose={onCancel}
      footer={
        <>
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            disabled={problems.length > 0 || test.status === "testing"}
            onClick={() => void runTest()}
          >
            {test.status === "testing" ? "Testing…" : "Test connection"}
          </Button>
          <Button
            variant="primary"
            disabled={problems.length > 0 || saving}
            onClick={() => void save()}
          >
            Save
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3 text-sm">
        <Field label="Name">
          <input
            className="input"
            value={draft.name}
            onChange={(e) => {
              change({ name: e.target.value });
            }}
            autoFocus
          />
        </Field>
        {driver.endpoint === "file" && (
          <Field label="Database file">
            <div className="flex gap-2">
              <input
                className="input flex-1 font-mono text-xs"
                value={draft.filePath ?? ""}
                placeholder="/path/to/database.db"
                onChange={(e) => {
                  change({ filePath: e.target.value });
                }}
              />
              <Button onClick={() => void chooseExisting()}>Browse…</Button>
              <Button onClick={() => void chooseNew()}>New file…</Button>
            </div>
          </Field>
        )}
        {driver.options.map((option) => (
          <OptionInput
            key={option.key}
            option={option}
            value={draft.options[option.key] ?? option.default ?? ""}
            onChange={(value) => {
              setOption(option.key, value);
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

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-xs text-zinc-400">{label}</span>
      {children}
    </label>
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
