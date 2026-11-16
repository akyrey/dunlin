import { useMemo, useState } from "react";

import type { KeySummary } from "../../ipc";
import { buildKeyTree, type KeyRow, visibleRows } from "./keyTree";

const TYPE_STYLE: Record<string, string> = {
  string: "text-emerald-300",
  hash: "text-sky-300",
  list: "text-amber-300",
  set: "text-fuchsia-300",
  zset: "text-rose-300",
  stream: "text-teal-300",
};
/** Pixels of indentation per tree level. */
const INDENT = 12;
const BASE_PADDING = 12;

type KeyTreeListProps = {
  keys: KeySummary[];
  onOpen: (summary: KeySummary) => void;
};

function toggled(set: ReadonlySet<string>, path: string): ReadonlySet<string> {
  const next = new Set(set);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  return next;
}

function KeyTreeRow({
  row,
  onToggle,
  onOpen,
}: {
  row: KeyRow;
  onToggle: (path: string) => void;
  onOpen: (summary: KeySummary) => void;
}) {
  const style = { paddingLeft: BASE_PADDING + row.depth * INDENT };
  const base =
    "flex w-full items-center gap-1.5 py-0.5 pr-3 text-left text-xs hover:bg-zinc-800/60";
  if (row.kind === "folder") {
    return (
      <button
        type="button"
        className={base}
        style={style}
        aria-expanded={row.expanded}
        aria-label={`${row.label}, ${row.count} ${row.count === 1 ? "key" : "keys"}`}
        title={`${row.path}*`}
        onClick={() => {
          onToggle(row.path);
        }}
      >
        <span className="w-3 shrink-0 text-[10px] text-zinc-500" aria-hidden="true">
          {row.expanded ? "▾" : "▸"}
        </span>
        <span className="truncate font-mono text-zinc-200">{row.label}</span>
        <span className="ml-auto shrink-0 text-[10px] text-zinc-500">{row.count}</span>
      </button>
    );
  }
  const { key, keyType } = row.summary;
  return (
    <button
      type="button"
      className={base}
      style={style}
      aria-label={key.name}
      title={key.name}
      onClick={() => {
        onOpen(row.summary);
      }}
    >
      <span
        className={`w-10 shrink-0 font-mono text-[10px] uppercase ${TYPE_STYLE[keyType] ?? "text-zinc-400"}`}
      >
        {keyType === "ReJSON-RL" ? "json" : keyType}
      </span>
      <span className="truncate font-mono">{row.label}</span>
    </button>
  );
}

/** Scanned keys as folders split on `:`, collapsed until opened. */
export function KeyTreeList({ keys, onOpen }: KeyTreeListProps) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const tree = useMemo(() => buildKeyTree(keys), [keys]);
  const rows = visibleRows(tree, expanded);
  const onToggle = (path: string) => {
    setExpanded((current) => toggled(current, path));
  };
  return rows.map((row) => (
    <li key={row.kind === "folder" ? `f:${row.path}` : `k:${row.summary.key.id}`}>
      <KeyTreeRow row={row} onToggle={onToggle} onOpen={onOpen} />
    </li>
  ));
}
