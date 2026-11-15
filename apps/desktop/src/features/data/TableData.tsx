import { Skeleton } from "../../components/Skeleton";
import { useEffect, useId, useMemo, useRef, useState } from "react";

import { Button } from "../../components/Button";
import {
  type AppError,
  type KeyArg,
  type RowCount,
  type SortKey,
  type TableInfo,
  type TablePage,
  type TableRef,
  type Value,
  cellFullValue,
  queryCancel,
  schemaTable,
  tabClose,
  tableCount,
  tablePage,
  toAppError,
} from "../../ipc";
import { serialQueue } from "../../lib/serial";
import { NO_TEXT_SUBSTITUTION, hasTypographicQuotes, straightenQuotes } from "../../lib/quotes";
import { ResultView } from "../grid/ResultView";
import { ChangesBar, EditToolbar } from "../edit/EditBars";
import { type EditingProfile, useTableEditing } from "../edit/useTableEditing";
import { tableKey } from "../workspace/workspaceStore";
import { loadFilters, rememberFilter } from "./filterHistory";
import { pageSizes, describeMatch, followTarget, gridColumns, nextSort, rowKeyOf } from "./model";
import { currentSettings } from "../settings/settingsStore";

type Props = {
  sessionId: string;
  connectionId: string;
  table: TableRef;
  match: KeyArg[];
  /** A foreign key cell was followed. */
  onFollow: (table: TableRef, match: KeyArg[]) => void;
  /** The workspace view this is, for the close and quit guards. */
  viewKey: string;
  /** The session is read-only: nothing can be edited. */
  readOnly: boolean;
  profile: EditingProfile;
};

type Query = {
  filter: string;
  sort: SortKey[];
  offset: number;
  pageSize: number;
  match: KeyArg[];
  /**
   * The cursor each page up to this one started after (keyset pagination); the last is this
   * page's. `null` entries start at `offset`: the first page, or an order that cannot seek.
   */
  trail: (Value[] | null)[];
};

const START: Query["trail"] = [null];

const formatCount = (count: RowCount | null): string => {
  if (!count || count.kind === "unknown") return "";
  const rows = Number(count.rows).toLocaleString("en-US");
  return count.kind === "exact" ? `${rows} rows` : `~${rows} rows`;
};

/** A table's rows a page at a time, sorted and filtered on the server (task 6.7). */
export function TableData({
  sessionId,
  connectionId,
  table,
  match: initialMatch,
  onFollow,
  viewKey,
  readOnly,
  profile,
}: Props) {
  const viewId = useMemo(() => crypto.randomUUID(), []);
  // Counts and whole cells use a second connection, so they never wait for (or block) a page.
  const auxId = `${viewId}:aux`;
  const filterId = useId();
  const historyKey = `${connectionId}.${tableKey(table)}`;
  const [query, setQuery] = useState<Query>({
    filter: "",
    sort: [],
    offset: 0,
    pageSize: currentSettings().defaultPageSize,
    match: initialMatch,
    trail: START,
  });
  const [draft, setDraft] = useState("");
  const [page, setPage] = useState<TablePage | null>(null);
  const [info, setInfo] = useState<TableInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<AppError | null>(null);
  const [count, setCount] = useState<RowCount | null>(null);
  const [counting, setCounting] = useState(false);
  const [recent, setRecent] = useState(() => loadFilters(historyKey));
  const [reload, setReload] = useState(0);
  const request = useRef(0);
  // Each of the view's connections runs one statement at a time.
  const pages = useMemo(() => serialQueue(), []);
  const aux = useMemo(() => serialQueue(), []);

  // The view's connections close with the view.
  useEffect(
    () => () => {
      for (const id of [viewId, `${viewId}:aux`]) {
        tabClose(sessionId, id).catch(() => {
          // The session may already be closed, which closed these connections too.
        });
      }
    },
    [sessionId, viewId],
  );

  useEffect(() => {
    let stale = false;
    schemaTable(sessionId, table).then(
      (described) => {
        if (!stale) setInfo(described);
      },
      () => {
        // Without the description the grid still works, just without key markers.
      },
    );
    return () => {
      stale = true;
    };
  }, [sessionId, table]);

  useEffect(() => {
    const id = ++request.current;
    const args = {
      sessionId,
      viewId,
      table,
      filter: query.filter || undefined,
      key: query.match,
      sort: query.sort,
      limit: query.pageSize,
      offset: query.offset,
      after: query.trail.at(-1) ?? undefined,
    };
    // A page still loading is superseded: cancel it, and start this one when it has ended.
    if (pages.busy()) {
      queryCancel(sessionId, viewId).catch(() => {
        // Nothing to cancel any more.
      });
    }
    pages
      .run(() => tablePage(args))
      .then(
        (loaded) => {
          if (id !== request.current) return;
          setPage(loaded);
          setError(null);
          setLoading(false);
        },
        (caught: unknown) => {
          if (id !== request.current) return;
          // The previous page stays visible under the error (task 6.8).
          setError(toAppError(caught));
          setLoading(false);
        },
      );
  }, [sessionId, viewId, table, query, reload, pages]);

  // An estimate right away for the unfiltered table; filtered counts are exact, on request.
  useEffect(() => {
    let stale = false;
    aux
      .run(() =>
        tableCount({
          sessionId,
          viewId: auxId,
          table,
          filter: query.filter || undefined,
          key: query.match,
          exact: false,
        }),
      )
      .then(
        (estimate) => {
          if (!stale) setCount(estimate);
        },
        () => {
          if (!stale) setCount(null);
        },
      );
    return () => {
      stale = true;
    };
  }, [sessionId, auxId, table, query.filter, query.match, aux]);

  const changeNow = (patch: Partial<Query>) => {
    setLoading(true);
    setQuery((current) => ({ ...current, offset: 0, trail: START, ...patch }));
  };
  const countExactly = async () => {
    setCounting(true);
    try {
      setCount(
        await aux.run(() =>
          tableCount({
            sessionId,
            viewId: auxId,
            table,
            filter: query.filter || undefined,
            key: query.match,
            exact: true,
          }),
        ),
      );
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setCounting(false);
    }
  };

  const columns = useMemo(() => gridColumns(page?.columns ?? [], info), [page, info]);
  const names = columns.map((c) => c.name);
  const rows = useMemo(() => page?.rows ?? [], [page]);
  const first = query.offset + 1;
  const last = query.offset + rows.length;
  const editing = useTableEditing({
    sessionId,
    viewId,
    viewKey,
    table,
    info,
    columns,
    page: rows,
    readOnly,
    profile,
    onApplied: () => {
      setLoading(true);
      setReload((n) => n + 1);
    },
  });
  const shownRows = useMemo(() => editing.rows.map((r) => r.values), [editing.rows]);

  /** Anything that loads another page first asks what to do with pending edits (task 7.13). */
  const change = (patch: Partial<Query>, action = "change what is shown") => {
    editing.guarded(action, () => {
      changeNow(patch);
    });
  };
  const applyFilter = (filter: string) => {
    setRecent(rememberFilter(historyKey, filter));
    change({ filter: filter.trim() }, "filter");
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-zinc-800 px-3 py-1.5 text-xs">
        <form
          className="flex min-w-64 flex-1 items-center gap-1"
          onSubmit={(e) => {
            e.preventDefault();
            applyFilter(draft);
          }}
        >
          <label htmlFor={filterId} className="font-mono text-zinc-500">
            WHERE
          </label>
          <input
            id={filterId}
            list={`${filterId}-recent`}
            className="input min-w-0 flex-1 py-0.5 font-mono text-xs"
            placeholder="a filter expression, e.g. total > 100"
            aria-label="Filter"
            {...NO_TEXT_SUBSTITUTION}
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value);
            }}
          />
          <datalist id={`${filterId}-recent`}>
            {recent.map((f) => (
              <option key={f} value={f} />
            ))}
          </datalist>
          <Button type="submit" className="py-0.5 text-xs">
            Apply
          </Button>
          {query.filter && (
            <Button
              variant="ghost"
              className="py-0.5 text-xs"
              onClick={() => {
                setDraft("");
                change({ filter: "" });
              }}
            >
              Clear
            </Button>
          )}
        </form>
        {query.match.length > 0 && (
          <span className="flex items-center gap-1 rounded bg-sky-950 px-2 py-0.5 text-sky-200">
            {describeMatch(query.match)}
            <button
              type="button"
              aria-label="Show every row"
              className="text-sky-400 hover:text-sky-100"
              onClick={() => {
                change({ match: [] });
              }}
            >
              ×
            </button>
          </span>
        )}
        <Button
          variant="ghost"
          className="py-0.5 text-xs"
          onClick={() => {
            editing.guarded("refresh", () => {
              setLoading(true);
              setReload((n) => n + 1);
            });
          }}
        >
          Refresh
        </Button>
        {loading && (
          <Button
            variant="danger"
            className="py-0.5 text-xs"
            onClick={() => void queryCancel(sessionId, viewId)}
          >
            Cancel
          </Button>
        )}
      </div>
      {hasTypographicQuotes(draft) && (
        <p className="flex items-center gap-2 border-b border-amber-900 bg-amber-950/50 px-3 py-1 text-xs text-amber-300">
          The filter has typographic quotes (“ ” ‘ ’), which SQL does not accept; macOS smart quotes
          may have typed them.
          <Button
            variant="ghost"
            className="px-1 py-0 text-xs"
            onClick={() => {
              setDraft(straightenQuotes(draft));
            }}
          >
            Use straight quotes
          </Button>
        </p>
      )}
      {editing.editable ? (
        <EditToolbar editing={editing} />
      ) : (
        page && (
          <p className="border-b border-zinc-800 px-3 py-1 text-xs text-zinc-500" role="note">
            {editing.reason}
          </p>
        )
      )}
      <ChangesBar editing={editing} />
      {error && (
        <p
          role="alert"
          className="border-b border-rose-900 bg-rose-950/50 px-3 py-1 font-mono text-xs text-rose-300"
        >
          {error.message}
        </p>
      )}
      <div className="min-h-0 flex-1">
        {page ? (
          <ResultView
            columns={columns}
            rows={shownRows}
            resetKey={page}
            exportSource={{
              sessionId,
              name: table.name,
              columns: page.columns,
              table: {
                viewId,
                table,
                filter: query.filter || undefined,
                key: query.match,
                sort: query.sort,
              },
            }}
            onSelectionChange={editing.setSelection}
            edit={editing.edit}
            rowOffset={query.offset}
            sort={query.sort}
            onSort={(column, additive) => {
              change({ sort: nextSort(query.sort, column, additive) }, "change the sort");
            }}
            onFollow={(cell) => {
              const row = rows[cell.row];
              const column = names[cell.col];
              const target = row && column ? followTarget(info, names, row, column) : null;
              if (target) onFollow(target.table, target.match);
            }}
            loadFull={(cell) => {
              const row = rows[cell.row];
              const column = names[cell.col];
              const key = row ? rowKeyOf(info, names, row) : null;
              if (!key || !column) return null;
              return () =>
                aux.run(() => cellFullValue({ sessionId, viewId: auxId, table, key, column }));
            }}
            noFullReason="The table has no primary key or unique not-null key, so the row cannot be fetched again."
            toolbar={
              <span className="ml-auto flex items-center gap-2">
                <span role="status" aria-label="Rows shown" className="text-zinc-400">
                  {rows.length === 0
                    ? "No rows"
                    : `Rows ${first.toLocaleString("en-US")}–${last.toLocaleString("en-US")}`}
                  {count && count.kind !== "unknown" && ` of ${formatCount(count)}`}
                </span>
                {count?.kind !== "exact" && (
                  <Button
                    variant="ghost"
                    className="px-1 py-0 text-xs"
                    disabled={counting}
                    onClick={() => void countExactly()}
                  >
                    {counting ? "Counting…" : "Count exactly"}
                  </Button>
                )}
                <Button
                  variant="ghost"
                  className="px-1 py-0 text-xs"
                  aria-label="Previous page"
                  disabled={query.offset === 0}
                  onClick={() => {
                    editing.guarded("change the page", () => {
                      setLoading(true);
                      setQuery((q) => ({
                        ...q,
                        offset: Math.max(0, q.offset - q.pageSize),
                        trail: q.trail.length > 1 ? q.trail.slice(0, -1) : START,
                      }));
                    });
                  }}
                >
                  ‹
                </Button>
                <Button
                  variant="ghost"
                  className="px-1 py-0 text-xs"
                  aria-label="Next page"
                  disabled={!page.hasMore}
                  onClick={() => {
                    editing.guarded("change the page", () => {
                      setLoading(true);
                      const cursor = page.cursor;
                      setQuery((q) => ({
                        ...q,
                        offset: q.offset + q.pageSize,
                        trail: [...q.trail, cursor],
                      }));
                    });
                  }}
                >
                  ›
                </Button>
                <select
                  aria-label="Page size"
                  className="rounded bg-zinc-800 px-1 py-0.5 text-zinc-200"
                  value={query.pageSize}
                  onChange={(e) => {
                    change({ pageSize: Number(e.target.value) }, "change the page size");
                  }}
                >
                  {pageSizes(query.pageSize).map((size) => (
                    <option key={size} value={size}>
                      {size} / page
                    </option>
                  ))}
                </select>
              </span>
            }
          />
        ) : (
          loading && <Skeleton rows={8} label="Loading rows" />
        )}
      </div>
      {editing.dialogs}
    </div>
  );
}
