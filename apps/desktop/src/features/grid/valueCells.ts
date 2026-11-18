import type { CellAdapter } from "@roost/grid";

import type { Value } from "../../ipc";
import { formatValue, rawText } from "../../lib/value";
import type { GridColumn } from "./DataGrid";
import { present } from "./present";

const PK = { text: "PK", className: "text-amber-300" };
const FK = { text: "FK", className: "text-sky-300" };

/** How Dunlin's database values look in @roost/grid; `nullText` is the user's NULL text. */
export function valueCells(nullText: string): CellAdapter<Value, GridColumn> {
  return {
    present: (value, column) => present(value, column.category, nullText),
    widthText: (value) => formatValue(value).text,
    copyText: (value) => (value.t === "null" ? null : rawText(value)),
    header: (column) => ({
      detail: column.typeName || column.category,
      badges: [...(column.primaryKey ? [PK] : []), ...(column.references ? [FK] : [])],
    }),
  };
}
