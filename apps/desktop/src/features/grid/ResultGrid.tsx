import { createColumnHelper, tableFeatures, useTable } from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useMemo, useRef } from "react";

import type { ColumnMeta, Value } from "../../ipc";
import { formatValue, isNumericCategory } from "../../lib/value";

const features = tableFeatures({});
const helper = createColumnHelper<typeof features, Value[]>();

const ROW_HEIGHT = 26;
const COLUMN_WIDTH = 180;
const ROW_NUMBER_WIDTH = 56;
const OVERSCAN = 12;

type Props = {
  columns: ColumnMeta[];
  rows: Value[][];
};

/** Read-only, row-virtualised result grid. Values render as text nodes only. */
export function ResultGrid({ columns, rows }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const tableColumns = useMemo(
    () =>
      helper.columns(
        columns.map((column, index) =>
          helper.accessor((row) => row[index] ?? { t: "null" }, {
            id: String(index),
            header: column.name,
          }),
        ),
      ),
    [columns],
  );
  const table = useTable({ features, columns: tableColumns, data: rows });
  const modelRows = table.getRowModel().rows;
  // The app does not use the React Compiler, so its memoisation warning does not apply.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: modelRows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
  });
  const template = `${ROW_NUMBER_WIDTH}px repeat(${columns.length}, ${COLUMN_WIDTH}px)`;
  const width = ROW_NUMBER_WIDTH + columns.length * COLUMN_WIDTH;

  return (
    <div
      ref={scrollRef}
      role="grid"
      aria-rowcount={rows.length + 1}
      className="h-full overflow-auto font-mono text-xs"
    >
      <div style={{ width }} className="sticky top-0 z-10">
        <div
          role="row"
          className="grid border-b border-zinc-700 bg-zinc-800"
          style={{ gridTemplateColumns: template }}
        >
          <div role="columnheader" className="px-2 py-1 text-right text-zinc-500">
            #
          </div>
          {columns.map((column, index) => (
            <div
              key={index}
              role="columnheader"
              title={column.typeName || column.category}
              className={`truncate px-2 py-1 font-semibold text-zinc-200 ${isNumericCategory(column.category) ? "text-right" : ""}`}
            >
              {column.name}
            </div>
          ))}
        </div>
      </div>
      <div style={{ height: virtualizer.getTotalSize(), width, position: "relative" }}>
        {virtualizer.getVirtualItems().map((item) => {
          const row = modelRows[item.index];
          if (!row) return null;
          return (
            <div
              key={row.id}
              role="row"
              aria-rowindex={item.index + 2}
              className="absolute left-0 grid border-b border-zinc-800/80 hover:bg-zinc-800/50"
              style={{
                gridTemplateColumns: template,
                height: ROW_HEIGHT,
                transform: `translateY(${item.start}px)`,
                width,
              }}
            >
              <div className="px-2 py-1 text-right text-zinc-600">{item.index + 1}</div>
              {row.getAllCells().map((cell) => (
                <Cell
                  key={cell.id}
                  value={cell.getValue() as Value}
                  meta={columns[Number(cell.column.id)]}
                />
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function Cell({ value, meta }: { value: Value; meta: ColumnMeta | undefined }) {
  const formatted = formatValue(value);
  const numeric = meta ? isNumericCategory(meta.category) : false;
  return (
    <div
      role="gridcell"
      title={formatted.text}
      className={`truncate px-2 py-1 ${numeric ? "text-right" : ""} ${formatted.isNull ? "italic text-zinc-500" : "text-zinc-100"}`}
    >
      {formatted.text}
    </div>
  );
}
