import type { PlanNode } from "../../ipc";

/** A query plan as an indented tree (deferred feature 3). Each step shows the server's
 * estimates, and a bar for its share of the total cost when the server reports costs. */
export function PlanView({ plan, sql }: { plan: PlanNode; sql: string }) {
  const total = plan.cost ?? null;
  return (
    <div className="flex h-full flex-col">
      <p
        className="truncate border-b border-zinc-800/70 px-3 py-1 font-mono text-[11px] text-zinc-500"
        title={sql}
      >
        EXPLAIN {sql}
      </p>
      <div className="min-h-0 flex-1 overflow-auto px-2 py-2">
        <ul role="tree" aria-label="Query plan" className="text-xs">
          <PlanStep node={plan} total={total} depth={0} />
        </ul>
      </div>
    </div>
  );
}

function number(value: number): string {
  return value >= 1000 ? Math.round(value).toLocaleString() : String(Math.round(value * 100) / 100);
}

function PlanStep({ node, total, depth }: { node: PlanNode; total: number | null; depth: number }) {
  const share = total && node.cost !== null ? Math.min(1, node.cost / total) : null;
  return (
    <li role="treeitem" aria-expanded={node.children.length > 0 ? true : undefined}>
      <div
        className="flex items-baseline gap-2 rounded px-1 py-0.5 hover:bg-zinc-800/60"
        style={{ paddingLeft: `${depth * 16 + 4}px` }}
      >
        <span className="font-medium text-zinc-100">{node.operation}</span>
        {node.relation && <span className="font-mono text-sky-300">{node.relation}</span>}
        <span className="ml-auto flex shrink-0 items-center gap-3 text-zinc-500">
          {node.estimatedRows !== null && <span>rows ≈ {number(node.estimatedRows)}</span>}
          {node.cost !== null && <span>cost {number(node.cost)}</span>}
          {share !== null && (
            <span
              aria-hidden="true"
              className="h-1.5 w-16 overflow-hidden rounded bg-zinc-800"
              title={`${Math.round(share * 100)}% of the total cost`}
            >
              <span
                className="block h-full bg-amber-500"
                style={{ width: `${Math.max(2, share * 100)}%` }}
              />
            </span>
          )}
        </span>
      </div>
      {node.details.length > 0 && (
        <ul
          className="font-mono text-[11px] text-zinc-400"
          style={{ paddingLeft: `${depth * 16 + 20}px` }}
        >
          {node.details.map((detail, i) => (
            <li key={i}>{detail}</li>
          ))}
        </ul>
      )}
      {node.children.length > 0 && (
        <ul role="group">
          {node.children.map((child, i) => (
            <PlanStep key={i} node={child} total={total} depth={depth + 1} />
          ))}
        </ul>
      )}
    </li>
  );
}
