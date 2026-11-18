/** Column widths for database values (task 6.5). */
import { columnWidth as kitColumnWidth } from "@roost/grid";

import type { Value } from "../../ipc";
import { formatValue } from "../../lib/value";

export { MAX_AUTO_WIDTH, MIN_WIDTH, widthFor } from "@roost/grid";

const widthText = (value: Value): string => formatValue(value).text;

export function columnWidth(header: string, rows: Value[][], index: number, sampleAll = false) {
  return kitColumnWidth(header, rows, index, widthText, sampleAll);
}
