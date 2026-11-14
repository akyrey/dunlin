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
  cellFullValue,
  queryCancel,
  schemaTable,
  tabClose,
  tableCount,
  tablePage,
  toAppError,
} from "../../ipc";
import { ResultView } from "../grid/ResultView";
import { tableKey } from "../workspace/workspaceStore";
import { loadFilters, rememberFilter } from "./filterHistory";
import {
  DEFAULT_PAGE_SIZE,
  PAGE_SIZES,
  describeMatch,
  followTarget,
  gridColumns,
  nextSort,
  rowKeyOf,
} from "./model";

type Props = {
  sessionId: string;
  connectionId: string;
  table: TableRef;
  match: KeyArg[];
  /** A foreign key cell was followed. */
  onFollow: (table: TableRef, match: KeyArg[]) => void;
};

type Query = { filter: string; sort: SortKey[]; offset: number; pageSize: number; match: KeyArg[] };

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
    pageSize: DEFAULT_PAGE_SIZE,
    match: initialMatch,
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
    };
    tablePage(args).then(
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
  }, [sessionId, viewId, table, query, reload]);

  // An estimate right away for the unfiltered table; filtered counts are exact, on request.
  useEffect(() => {
    let stale = false;
    tableCount({
      sessionId,
      viewId: auxId,
      table,
      filter: query.filter || undefined,
      key: query.match,
      exact: false,
    }).then(
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
  }, [sessionId, auxId, table, query.filter, query.match]);

  const change = (patch: Partial<Query>) => {
    setLoading(true);
    setQuery((current) => ({ ...current, offset: 0, ...patch }));
  };
  const applyFilter = (filter: string) => {
    setRecent(rememberFilter(historyKey, filter));
    change({ filter: filter.trim() });
  };
  const countExactly = async () => {
    setCounting(true);
    try {
      setCount(
        await tableCount({
          sessionId,
          viewId: auxId,
          table,
          filter: query.filter || undefined,
          key: query.match,
          exact: true,
        }),
      );
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setCounting(false);
    }
  };

  const columns = useMemo(() => gridColumns(page?.columns ?? [], info), [page, info]);
  const names = columns.map((c) => c.name);
  const rows = page?.rows ?? [];
  const first = query.offset + 1;
  const last = query.offset + rows.length;

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
            setLoading(true);
            setReload((n) => n + 1);
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
            rows={rows}
            rowOffset={query.offset}
            sort={query.sort}
            onSort={(column, additive) => {
              change({ sort: nextSort(query.sort, column, additive) });
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
              return () => cellFullValue({ sessionId, viewId: auxId, table, key, column });
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
                    setLoading(true);
                    setQuery((q) => ({ ...q, offset: Math.max(0, q.offset - q.pageSize) }));
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
                    setLoading(true);
                    setQuery((q) => ({ ...q, offset: q.offset + q.pageSize }));
                  }}
                >
                  ›
                </Button>
                <select
                  aria-label="Page size"
                  className="rounded bg-zinc-800 px-1 py-0.5 text-zinc-200"
                  value={query.pageSize}
                  onChange={(e) => {
                    change({ pageSize: Number(e.target.value) });
                  }}
                >
                  {PAGE_SIZES.map((size) => (
                    <option key={size} value={size}>
                      {size} / page
                    </option>
                  ))}
                </select>
              </span>
            }
          />
        ) : (
          <p className="p-4 text-sm text-zinc-500">{loading ? "Loading…" : ""}</p>
        )}
      </div>
    </div>
  );
}
