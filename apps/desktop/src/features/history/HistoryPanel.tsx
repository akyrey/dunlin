import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@roost/ui";
import { type AppError, type HistoryEntry, historyClear, historyList, toAppError } from "../../ipc";

const PAGE = 50;
/** A click waits this long, so a double-click opens a tab without also inserting. */
const DOUBLE_CLICK_MS = 250;

type Props = {
  connectionId: string;
  /** Click: add the statement to the current editor. */
  onInsert: (statement: string) => void;
  /** Double-click: open the statement in a new tab. */
  onOpen: (statement: string) => void;
  onClose: () => void;
};

function outcomeOf(entry: HistoryEntry): string {
  const time = `${entry.durationMs} ms`;
  switch (entry.outcome) {
    case "rows":
      return `${entry.rowCount ?? 0}${entry.truncated ? "+" : ""} rows · ${time}`;
    case "affected":
      return `${entry.rowCount ?? 0} affected · ${time}`;
    case "error":
      return `failed · ${time}`;
  }
}

/** The connection's query history, newest first, searchable, a page at a time (task 5.12). */
export function HistoryPanel({ connectionId, onInsert, onOpen, onClose }: Props) {
  const [search, setSearch] = useState("");
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const request = useRef(0);
  const pendingClick = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cancelClick = () => {
    if (pendingClick.current !== null) clearTimeout(pendingClick.current);
    pendingClick.current = null;
  };
  useEffect(() => cancelClick, []);

  const load = useCallback(
    async (before: number | undefined) => {
      const id = ++request.current;
      try {
        const page = await historyList({
          connectionId,
          search: search || undefined,
          limit: PAGE,
          before,
        });
        if (id !== request.current) return;
        setEntries((current) => (before === undefined ? page : [...current, ...page]));
        setMore(page.length === PAGE);
        setError(null);
      } catch (caught) {
        if (id === request.current) setError(toAppError(caught));
      }
    },
    [connectionId, search],
  );

  useEffect(() => {
    const timer = setTimeout(() => void load(undefined), 150);
    return () => {
      clearTimeout(timer);
    };
  }, [load]);

  const clear = async () => {
    try {
      await historyClear(connectionId);
      setEntries([]);
      setMore(false);
    } catch (caught) {
      setError(toAppError(caught));
    }
  };

  return (
    <aside
      aria-label="History"
      className="flex h-full w-80 shrink-0 flex-col border-l border-zinc-800 bg-zinc-900"
    >
      <div className="flex items-center gap-2 border-b border-zinc-800 px-2 py-1.5">
        <input
          className="input min-w-0 flex-1 py-0.5 text-xs"
          placeholder="Search history"
          aria-label="Search history"
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
          }}
        />
        <Button variant="ghost" className="px-1 text-xs" onClick={() => void load(undefined)}>
          Reload
        </Button>
        <Button variant="ghost" className="px-1 text-xs" onClick={() => void clear()}>
          Clear
        </Button>
        <Button
          variant="ghost"
          className="px-1 text-xs"
          aria-label="Close history"
          onClick={onClose}
        >
          ×
        </Button>
      </div>
      {error && (
        <p role="alert" className="px-2 py-1 text-xs text-rose-300">
          {error.message}
        </p>
      )}
      <ul aria-label="History entries" className="min-h-0 flex-1 overflow-auto">
        {entries.map((entry) => (
          <li key={entry.id} className="border-b border-zinc-800/70">
            <button
              type="button"
              className="w-full px-2 py-1 text-left hover:bg-zinc-800/60"
              title="Click to insert, double-click to open in a new tab"
              onClick={() => {
                cancelClick();
                pendingClick.current = setTimeout(() => {
                  pendingClick.current = null;
                  onInsert(entry.statement);
                }, DOUBLE_CLICK_MS);
              }}
              onDoubleClick={() => {
                cancelClick();
                onOpen(entry.statement);
              }}
            >
              <span className="line-clamp-2 font-mono text-xs text-zinc-200">
                {entry.statement}
              </span>
              <span
                className={`block text-[10px] ${entry.outcome === "error" ? "text-rose-400" : "text-zinc-500"}`}
              >
                {new Date(entry.startedAt).toLocaleString()} · {outcomeOf(entry)}
              </span>
            </button>
          </li>
        ))}
        {entries.length === 0 && !error && (
          <li className="px-2 py-2 text-xs text-zinc-500">No statements yet.</li>
        )}
      </ul>
      {more && (
        <Button
          variant="ghost"
          className="m-1 text-xs"
          onClick={() => void load(entries.at(-1)?.id)}
        >
          Load more
        </Button>
      )}
    </aside>
  );
}
