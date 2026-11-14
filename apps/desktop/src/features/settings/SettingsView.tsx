import { type ReactNode, useState } from "react";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { toast } from "../../components/toastStore";
import { type AppError, type Settings, logsDir, revealFile, toAppError } from "../../ipc";
import { BOUNDS } from "./defaults";
import { NUMBER_LABELS, type NumberField, settingsProblem } from "./validate";
import { useSettings } from "./settingsStore";
import { useStartup } from "./startup";

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  // Each control carries its own aria-label; this text is for the eye.
  return (
    <div className="grid grid-cols-[10rem_1fr] items-center gap-x-3 gap-y-0.5">
      <span className="text-zinc-300">{label}</span>
      {children}
      {hint && <span className="col-start-2 text-xs text-zinc-500">{hint}</span>}
    </div>
  );
}

type Props = { onClose: () => void; onShowTrustedHosts: () => void };

/** Settings (task 8.8). Changes are kept as a draft until Save. */
export function SettingsView({ onClose, onShowTrustedHosts }: Props) {
  const saved = useSettings((s) => s.settings);
  const loadError = useSettings((s) => s.error);
  const startupMs = useStartup((s) => s.startupMs);
  const [draft, setDraft] = useState<Settings>(saved);
  const [error, setError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
  const problem = settingsProblem(draft);

  const change = (patch: Partial<Settings>) => {
    setDraft((current) => ({ ...current, ...patch }));
  };
  const number = (field: NumberField) => (
    <input
      aria-label={NUMBER_LABELS[field]}
      type="number"
      className="input w-32"
      min={BOUNDS[field].min}
      max={BOUNDS[field].max}
      value={Number.isNaN(draft[field]) ? "" : draft[field]}
      onChange={(e) => {
        change({ [field]: e.target.valueAsNumber });
      }}
    />
  );

  const save = async () => {
    setSaving(true);
    const failure = await useSettings.getState().save(draft);
    setSaving(false);
    if (failure) setError(failure);
    else {
      toast.success("Settings saved");
      onClose();
    }
  };

  const openLogs = async () => {
    try {
      await revealFile(await logsDir());
    } catch (caught) {
      setError(toAppError(caught));
    }
  };

  const shownError = error ?? loadError;
  return (
    <Modal
      title="Settings"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            disabled={problem !== null || saving}
            onClick={() => void save()}
          >
            Save
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-3 text-sm"
        onSubmit={(e) => {
          e.preventDefault();
          if (problem === null) void save();
        }}
      >
        <Row label="Theme">
          <select
            aria-label="Theme"
            className="input w-40"
            value={draft.theme}
            onChange={(e) => {
              change({ theme: e.target.value as Settings["theme"] });
            }}
          >
            <option value="system">Same as the system</option>
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </Row>
        <Row label="Editor font">
          <input
            aria-label="Editor font"
            className="input"
            value={draft.editorFontFamily}
            onChange={(e) => {
              change({ editorFontFamily: e.target.value });
            }}
          />
        </Row>
        <Row label="Editor font size">{number("editorFontSize")}</Row>
        <Row label="Default row limit" hint="For new editor tabs.">
          {number("defaultRowLimit")}
        </Row>
        <Row label="Default page size" hint="For newly opened tables.">
          {number("defaultPageSize")}
        </Row>
        <Row label="History size" hint="Statements kept per connection; 0 keeps none.">
          {number("historyCap")}
        </Row>
        <Row label="Preview before apply">
          <select
            aria-label="Preview before apply"
            className="input w-56"
            value={draft.previewPolicy}
            onChange={(e) => {
              change({ previewPolicy: e.target.value as Settings["previewPolicy"] });
            }}
          >
            <option value="once_per_session">Until skipped for the session</option>
            <option value="always">Always</option>
          </select>
        </Row>
        <Row label="NULL shown as">
          <input
            aria-label="NULL shown as"
            className="input w-32"
            value={draft.nullText}
            onChange={(e) => {
              change({ nullText: e.target.value });
            }}
          />
        </Row>
        <div className="flex gap-2 border-t border-zinc-800 pt-3">
          <Button variant="ghost" onClick={onShowTrustedHosts}>
            Trusted SSH hosts…
          </Button>
          <Button variant="ghost" onClick={() => void openLogs()}>
            Open logs folder
          </Button>
          {startupMs !== null && (
            <span className="ml-auto self-center text-xs text-zinc-500">
              Started in {startupMs} ms
            </span>
          )}
        </div>
        {(problem ?? shownError) && (
          <p role="alert" className="text-xs text-rose-300">
            {problem ?? shownError?.message}
          </p>
        )}
      </form>
    </Modal>
  );
}
