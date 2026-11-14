/** How a value looks in a grid cell, by its kind and the column's category (task 6.6). Pure;
 * the result is always rendered as a text node, never as HTML. */
import type { TypeCategory, Value } from "../../ipc";
import { formatSize, formatValue, isNumericCategory } from "../../lib/value";

export type CellKind =
  "null" | "empty" | "true" | "false" | "number" | "json" | "binary" | "array" | "text";

export type Presentation = {
  kind: CellKind;
  /** The preview shown in the cell. */
  text: string;
  /** The whole value is longer than the preview (or than what the backend sent). */
  cut: boolean;
  alignRight: boolean;
};

/** Characters of a long value shown in a cell. */
const PREVIEW_CHARS = 200;

function preview(text: string, truncated: boolean): { text: string; cut: boolean } {
  const newline = text.search(/[\r\n]/);
  const firstLine = newline >= 0 ? text.slice(0, newline) : text;
  const short = firstLine.length > PREVIEW_CHARS ? firstLine.slice(0, PREVIEW_CHARS) : firstLine;
  const cut = truncated || short.length < text.length;
  return { text: cut ? `${short}…` : short, cut };
}

export function present(value: Value, category: TypeCategory): Presentation {
  const base = { cut: false, alignRight: false };
  switch (value.t) {
    case "null":
      return { ...base, kind: "null", text: "NULL" };
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

export const KIND_CLASS: Record<CellKind, string> = {
  null: "italic text-zinc-500",
  empty: "",
  true: "text-emerald-400",
  false: "text-rose-400",
  number: "tabular-nums text-sky-200",
  json: "text-amber-200",
  binary: "italic text-violet-300",
  array: "text-teal-200",
  text: "text-zinc-100",
};
