/** How a value looks in a grid cell, by its kind and the column's category (task 6.6). Pure;
 * the result is always rendered as a text node, never as HTML. */
import { type Presentation, preview } from "@roost/grid";

import type { TypeCategory, Value } from "../../ipc";
import { formatSize, formatValue, isNumericCategory } from "../../lib/value";

/** `nullText` is the user's `NULL` display text (Settings). */
export function present(value: Value, category: TypeCategory, nullText = "NULL"): Presentation {
  const base = { cut: false, alignRight: false };
  switch (value.t) {
    case "null":
      return { ...base, kind: "null", text: nullText };
    case "bool":
      return { ...base, kind: value.v ? "true" : "false", text: String(value.v) };
    case "bytes":
      return { ...base, kind: "binary", text: `<binary ${formatSize(value.len)}>`, cut: true };
    case "array":
      return { ...base, kind: "array", ...preview(formatValue(value).text, false) };
    case "json": {
      // One line: indentation and line breaks would hide everything but the first brace.
      const compact = value.v.replace(/\s*\n\s*/g, " ");
      return { ...base, kind: "json", ...preview(compact, value.truncated ?? false) };
    }
    case "text":
    case "xml":
      if (value.v === "" && !value.truncated) return { ...base, kind: "empty", text: "" };
      return { ...base, kind: "text", ...preview(value.v, value.truncated ?? false) };
    case "int":
    case "uint":
    case "float":
    case "decimal":
      return { ...base, kind: "number", text: value.v, alignRight: true };
    default: {
      const text = formatValue(value).text;
      return { ...base, kind: "text", text, alignRight: isNumericCategory(category) };
    }
  }
}
