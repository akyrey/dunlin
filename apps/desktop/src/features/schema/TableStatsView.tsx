/** The Statistics sub-tab of a table (Phase 10): sizes, row estimate, maintenance and how often
 * each index is used. Shows only what the server reports. */
import type { IndexStats, TableStats } from "../../ipc";

function bytes(value: number | null): string | null {
  if (value === null) return null;
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${unit === 0 ? size : size.toFixed(1)} ${units[unit]}`;
}

const count = (value: number | null) => (value === null ? null : value.toLocaleString());

export function TableStatsView({ stats }: { stats: TableStats }) {
  const facts: [string, string | null][] = [
    ["Total size", bytes(stats.totalBytes)],
    ["Table", bytes(stats.tableBytes)],
    ["Indexes", bytes(stats.indexBytes)],
    ["Rows (estimate)", count(stats.estimatedRows)],
    ["Dead rows", count(stats.deadRows)],
    ["Last vacuum", stats.lastVacuum],
    ["Last analyze", stats.lastAnalyze],
    ["Last change", stats.updatedAt],
  ];
  const known = facts.filter((f): f is [string, string] => f[1] !== null);
  const usage = stats.indexes.some((i) => i.scans !== null);
  return (
    <div className="flex flex-col gap-4 p-4 text-sm">
      {known.length > 0 ? (
        <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-1">
          {known.map(([label, value]) => (
            <div key={label} className="contents">
              <dt className="text-zinc-400">{label}</dt>
              <dd className="font-mono text-xs text-zinc-200">{value}</dd>
            </div>
          ))}
        </dl>
      ) : (
        <p className="text-zinc-500">This database reports no sizes or counts for the table.</p>
      )}
      <section aria-label="Index usage">
        <h3 className="mb-1 text-xs font-semibold text-zinc-300">Indexes</h3>
        {stats.indexes.length === 0 ? (
          <p className="text-zinc-500">No indexes.</p>
        ) : (
          <table className="w-full text-left text-xs">
            <thead className="text-zinc-400">
              <tr>
                <th className="py-1 font-medium">Name</th>
                <th className="py-1 font-medium">Size</th>
                {usage && <th className="py-1 font-medium">Scans</th>}
              </tr>
            </thead>
            <tbody>
              {stats.indexes.map((index) => (
                <IndexRow key={index.name} index={index} usage={usage} />
              ))}
            </tbody>
          </table>
        )}
        {usage && (
          <p className="mt-1 text-[11px] text-zinc-500">
            Scans are counted
            {stats.countedSince
              ? ` since ${stats.countedSince}`
              : " since the statistics were last reset"}
            . An index with none since then may not be needed, but check before dropping it.
          </p>
        )}
      </section>
    </div>
  );
}

function IndexRow({ index, usage }: { index: IndexStats; usage: boolean }) {
  return (
    <tr aria-label={`Index ${index.name}`} className="border-t border-zinc-800/70">
      <td className="py-1 font-mono">
        {index.name}
        {index.scans === 0 && (
          <span className="ml-2 rounded bg-amber-900 px-1 text-[10px] text-amber-100">unused</span>
        )}
      </td>
      <td className="py-1">{bytes(index.bytes) ?? "—"}</td>
      {usage && <td className="py-1">{count(index.scans) ?? "—"}</td>}
    </tr>
  );
}
