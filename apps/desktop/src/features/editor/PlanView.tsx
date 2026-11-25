import { Button, toast } from "@roost/ui";

import {
  type HintKind,
  type PlanNode,
  type PlanReport,
  type ReportedHint,
  toAppError,
} from "../../ipc";
import { stageSuggestedIndex } from "../schema/stageIndex";

/** A query plan as an indented tree (deferred feature 3, D-043). Each step shows the server's
 * estimates and, when measured, the actual rows, loops and time, with a bar for its share of
 * the total time (or cost). Hints are marked on their steps. */
export function PlanView({
  report,
  sql,
  analyzed,
  sessionId,
}: {
  report: PlanReport;
  sql: string;
  analyzed: boolean;
  sessionId: string;
}) {
  const { plan } = report;
  const total = analyzed ? (plan.actualTimeMs ?? report.totalMs) : plan.cost;
  const byPath = new Map<string, ReportedHint[]>();
  for (const hint of report.hints) {
    const key = hint.path.join(".");
    byPath.set(key, [...(byPath.get(key) ?? []), hint]);
  }
  return (
    <div className="flex h-full flex-col">
      <p
        className="flex gap-2 truncate border-b border-zinc-800/70 px-3 py-1 font-mono text-[11px] text-zinc-500"
        title={sql}
      >
        <span className="truncate">
          {analyzed ? "EXPLAIN ANALYZE" : "EXPLAIN"} {sql}
        </span>
        {report.totalMs !== null && (
          <span className="ml-auto shrink-0 text-zinc-300">{ms(report.totalMs)} in all</span>
        )}
      </p>
      <div className="min-h-0 flex-1 overflow-auto px-2 py-2">
        <ul role="tree" aria-label="Query plan" className="text-xs">
          <PlanStep
            node={plan}
            path={[]}
            context={{ total: total ?? null, measured: analyzed, byPath, sessionId }}
          />
        </ul>
      </div>
    </div>
  );
}

type Context = {
  total: number | null;
  measured: boolean;
  byPath: Map<string, ReportedHint[]>;
  sessionId: string;
};

function number(value: number): string {
  return value >= 1000 ? Math.round(value).toLocaleString() : String(Math.round(value * 100) / 100);
}

function ms(value: number): string {
  return value >= 1000 ? `${(value / 1000).toFixed(2)} s` : `${number(value)} ms`;
}

const HINT_LABEL: Record<HintKind, string> = {
  hottest: "slowest",
  misestimate: "misestimate",
  full_scan: "full scan",
  spill: "spill",
  many_loops: "many loops",
};

function PlanStep({ node, path, context }: { node: PlanNode; path: number[]; context: Context }) {
  const { total, measured } = context;
  const value = measured ? node.actualTimeMs : node.cost;
  const share = total && value !== null ? Math.min(1, value / total) : null;
  const hints = context.byPath.get(path.join(".")) ?? [];
  return (
    <li role="treeitem" aria-expanded={node.children.length > 0 ? true : undefined}>
      <div
        className="flex items-baseline gap-2 rounded px-1 py-0.5 hover:bg-zinc-800/60"
        style={{ paddingLeft: `${path.length * 16 + 4}px` }}
      >
        <span className="font-medium text-zinc-100">{node.operation}</span>
        {node.relation && <span className="font-mono text-sky-300">{node.relation}</span>}
        {hints.map((h, i) => (
          <span
            key={i}
            className={`rounded px-1 text-[10px] ${h.kind === "hottest" ? "bg-rose-900 text-rose-100" : "bg-amber-900 text-amber-100"}`}
          >
            {HINT_LABEL[h.kind]}
          </span>
        ))}
        <span className="ml-auto flex shrink-0 items-center gap-3 text-zinc-500">
          {node.estimatedRows !== null && <span>rows ≈ {number(node.estimatedRows)}</span>}
          {node.actualRows !== null && (
            <span className="text-zinc-300">
              rows {number(node.actualRows)}
              {node.loops !== null && node.loops > 1 && ` × ${number(node.loops)}`}
            </span>
          )}
          {node.actualTimeMs !== null && (
            <span className="text-zinc-300">{ms(node.actualTimeMs)}</span>
          )}
          {!measured && node.cost !== null && <span>cost {number(node.cost)}</span>}
          {share !== null && (
            <span
              aria-hidden="true"
              className="h-1.5 w-16 overflow-hidden rounded bg-zinc-800"
              title={`${Math.round(share * 100)}% of the total ${measured ? "time" : "cost"}`}
            >
              <span
                className={`block h-full ${measured ? "bg-rose-500" : "bg-amber-500"}`}
                style={{ width: `${Math.max(2, share * 100)}%` }}
              />
            </span>
          )}
        </span>
      </div>
      {(node.details.length > 0 || hints.length > 0) && (
        <ul className="text-[11px]" style={{ paddingLeft: `${path.length * 16 + 20}px` }}>
          {node.details.map((detail, i) => (
            <li key={i} className="font-mono text-zinc-400">
              {detail}
            </li>
          ))}
          {hints.map((hint, i) => (
            <HintLine key={`hint-${i}`} hint={hint} sessionId={context.sessionId} />
          ))}
        </ul>
      )}
      {node.children.length > 0 && (
        <ul role="group">
          {node.children.map((child, i) => (
            <PlanStep key={i} node={child} path={[...path, i]} context={context} />
          ))}
        </ul>
      )}
    </li>
  );
}

function HintLine({ hint, sessionId }: { hint: ReportedHint; sessionId: string }) {
  const suggested = hint.index;
  return (
    <li className="flex items-center gap-2 text-amber-200" aria-label={`Hint: ${hint.message}`}>
      <span>{hint.message}</span>
      {suggested && (
        <Button
          variant="ghost"
          className="py-0 text-[11px] text-sky-300"
          title={`Stage an index on ${suggested.table.name} (${suggested.index.columns.join(", ")}); it is created only when you apply it`}
          onClick={() => {
            stageSuggestedIndex(sessionId, suggested).catch((caught: unknown) => {
              toast.error(toAppError(caught).message);
            });
          }}
        >
          Create index on {suggested.index.columns.join(", ")}…
        </Button>
      )}
    </li>
  );
}
