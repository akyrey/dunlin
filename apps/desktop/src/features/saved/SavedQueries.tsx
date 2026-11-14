import { useState } from "react";

import { ConfirmDialog } from "../../components/ConfirmDialog";
import { type SavedQuery, toAppError } from "../../ipc";
import { openSavedQuery } from "./openSaved";
import { queriesFor, useSavedQueries } from "./savedQueriesStore";

type Props = { activeConnectionId: string | null };

/** The sidebar list: the active connection's queries and those for every connection (8.7). */
export function SavedQueries({ activeConnectionId }: Props) {
  const { queries, error: loadError } = useSavedQueries();
  const [renaming, setRenaming] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<SavedQuery | null>(null);
  const [error, setError] = useState<string | null>(null);
  const shown = queriesFor(queries, activeConnectionId);

  const open = async (query: SavedQuery) => {
    setError(null);
    setError(await openSavedQuery(query, activeConnectionId));
  };

  const rename = async (query: SavedQuery, name: string) => {
    setRenaming(null);
    if (!name.trim() || name.trim() === query.name) return;
    try {
      await useSavedQueries.getState().save({ ...query, name });
    } catch (caught) {
      setError(toAppError(caught).message);
    }
  };

  return (
    <section aria-label="Saved queries" className="border-t border-zinc-800 px-1 py-1">
      <h2 className="px-2 text-[11px] font-semibold uppercase tracking-wide text-zinc-500">
        Saved queries
      </h2>
      {(error ?? loadError) && (
        <p role="alert" className="px-2 text-xs text-rose-300">
          {error ?? loadError?.message}
        </p>
      )}
      {shown.length === 0 && (
        <p className="px-2 py-1 text-xs text-zinc-500">Save a tab's SQL with ⌘S.</p>
      )}
      <ul className="max-h-48 overflow-auto">
        {shown.map((query) => (
          <li
            key={query.id}
            className="group flex items-center gap-1 px-2 py-0.5 text-sm hover:bg-zinc-800/60"
          >
            {renaming === query.id ? (
              <input
                aria-label="Query name"
                autoFocus
                defaultValue={query.name}
                className="input min-w-0 flex-1 py-0 text-xs"
                onBlur={(e) => void rename(query, e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") e.currentTarget.blur();
                  if (e.key === "Escape") setRenaming(null);
                }}
              />
            ) : (
              <button
                type="button"
                className="min-w-0 flex-1 truncate text-left"
                title={query.sql}
                onClick={() => void open(query)}
              >
                {query.name}
                {query.connectionId === null && (
                  <span className="ml-1 text-[10px] text-zinc-500">all</span>
                )}
              </button>
            )}
            <span className="hidden gap-1 text-xs group-hover:flex group-focus-within:flex">
              <button
                type="button"
                aria-label={`Rename ${query.name}`}
                className="text-zinc-400 hover:text-zinc-100"
                onClick={() => {
                  setRenaming(query.id);
                }}
              >
                Rename
              </button>
              <button
                type="button"
                aria-label={`Delete ${query.name}`}
                className="text-zinc-400 hover:text-rose-300"
                onClick={() => {
                  setDeleting(query);
                }}
              >
                Delete
              </button>
            </span>
          </li>
        ))}
      </ul>
      {deleting && (
        <ConfirmDialog
          title="Delete saved query?"
          message={`"${deleting.name}" will be removed. Open tabs keep their SQL.`}
          confirmLabel="Delete"
          onConfirm={() => {
            const query = deleting;
            setDeleting(null);
            useSavedQueries
              .getState()
              .remove(query.id)
              .catch((caught: unknown) => {
                setError(toAppError(caught).message);
              });
          }}
          onCancel={() => {
            setDeleting(null);
          }}
        />
      )}
    </section>
  );
}
