/** The server's heaviest statements (Phase 10): from `pg_stat_statements` or
 * `performance_schema`, or how to turn recording on. A statement opens in a new editor tab. */
import { Button, Modal, Skeleton } from "@roost/ui";
import { useEffect, useState } from "react";

import { type AppError, type QueryStats, sessionQueryStats, toAppError } from "../../ipc";
import { useEditor } from "../editor/editorStore";

type Loaded =
  | { status: "loading" }
  | { status: "loaded"; stats: QueryStats }
  | { status: "failed"; error: AppError };

function ms(value: number): string {
  return value >= 1000 ? `${(value / 1000).toFixed(2)} s` : `${value.toFixed(2)} ms`;
}

export function QueryStatsDialog({
  sessionId,
  onClose,
}: {
  sessionId: string;
  onClose: () => void;
}) {
  const [loaded, setLoaded] = useState<Loaded>({ status: "loading" });
  useEffect(() => {
    let stale = false;
    sessionQueryStats(sessionId).then(
      (stats) => {
        if (!stale) setLoaded({ status: "loaded", stats });
      },
      (caught: unknown) => {
        if (!stale) setLoaded({ status: "failed", error: toAppError(caught) });
      },
    );
    return () => {
      stale = true;
    };
  }, [sessionId]);

  return (
    <Modal
      title="Query statistics"
      onClose={onClose}
      footer={
        <Button onClick={onClose} autoFocus>
          Close
        </Button>
      }
    >
      <Body
        loaded={loaded}
        onOpen={(statement) => {
          useEditor.getState().openTab(sessionId, { text: statement });
          onClose();
        }}
      />
    </Modal>
  );
}

function Body({ loaded, onOpen }: { loaded: Loaded; onOpen: (statement: string) => void }) {
  if (loaded.status === "loading") return <Skeleton rows={6} label="Loading query statistics" />;
  if (loaded.status === "failed") {
    return (
      <p role="alert" className="text-sm text-rose-300">
        {loaded.error.message}
      </p>
    );
  }
  const { stats } = loaded;
  if (stats.kind === "unavailable") {
    return <p className="text-sm text-amber-200">{stats.reason}</p>;
  }
  if (stats.statements.length === 0) {
    return <p className="text-sm text-zinc-400">The server has not recorded any statements yet.</p>;
  }
  return (
    <div className="max-h-[60vh] overflow-auto">
      <p className="mb-2 text-xs text-zinc-400">
        The statements with the most total time, as the server records them.
      </p>
      <table className="w-full text-left text-xs">
        <thead className="sticky top-0 bg-zinc-900 text-zinc-400">
          <tr>
            <th className="py-1 font-medium">Statement</th>
            <th className="py-1 text-right font-medium">Calls</th>
            <th className="py-1 text-right font-medium">Total</th>
            <th className="py-1 text-right font-medium">Mean</th>
          </tr>
        </thead>
        <tbody>
          {stats.statements.map((s, i) => (
            <tr key={i} className="border-t border-zinc-800/70 align-top">
              <td className="max-w-[16rem] py-1">
                <button
                  type="button"
                  className="block w-full truncate text-left font-mono text-sky-300 hover:underline"
                  title={`Open in a new editor tab:\n${s.statement}`}
                  onClick={() => {
                    onOpen(s.statement);
                  }}
                >
                  {s.statement}
                </button>
              </td>
              <td className="py-1 text-right">{s.calls.toLocaleString()}</td>
              <td className="py-1 text-right">{ms(s.totalMs)}</td>
              <td className="py-1 text-right">{ms(s.meanMs)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
