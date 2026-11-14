import { useId, useState } from "react";

import { MAX_ROW_LIMIT } from "./editorStore";

const PRESETS = [100, 1000, 10_000];

/** The tab's row limit: a preset, or a custom number (task 5.14). */
export function RowLimitSelect({
  value,
  onChange,
}: {
  value: number;
  onChange: (limit: number) => void;
}) {
  const id = useId();
  const [custom, setCustom] = useState(!PRESETS.includes(value));
  const [draft, setDraft] = useState(String(value));
  const commit = () => {
    const limit = Number(draft);
    if (Number.isInteger(limit) && limit >= 1 && limit <= MAX_ROW_LIMIT) onChange(limit);
    else setDraft(String(value));
  };
  return (
    <span className="flex items-center gap-1 text-xs text-zinc-400">
      <label htmlFor={id}>Row limit</label>
      <select
        id={id}
        className="rounded bg-zinc-800 px-1 py-0.5 text-zinc-200"
        value={custom ? "custom" : String(value)}
        onChange={(e) => {
          if (e.target.value === "custom") {
            setCustom(true);
            return;
          }
          setCustom(false);
          onChange(Number(e.target.value));
        }}
      >
        {PRESETS.map((preset) => (
          <option key={preset} value={preset}>
            {preset.toLocaleString("en-US")}
          </option>
        ))}
        <option value="custom">Custom…</option>
      </select>
      {custom && (
        <input
          aria-label="Custom row limit"
          type="number"
          min={1}
          max={MAX_ROW_LIMIT}
          className="w-20 rounded bg-zinc-800 px-1 py-0.5 text-zinc-200"
          value={draft}
          onChange={(e) => {
            setDraft(e.target.value);
          }}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
          }}
        />
      )}
    </span>
  );
}
