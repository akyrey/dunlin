/** Copying a selected range of database values as tab-separated values (task 6.5). */
import { type Range, toTsv as kitToTsv } from "@roost/grid";

import type { Value } from "../../ipc";
import { rawText } from "../../lib/value";

const copyText = (value: Value): string | null => (value.t === "null" ? null : rawText(value));

export function toTsv(rows: Value[][], range: Range, header?: string[]): string {
  return kitToTsv(rows, range, copyText, header);
}
