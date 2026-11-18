/** Which editor a cell gets, and how its text becomes a `Value` (task 7.7). Pure. */
import type { EditorKind, Parsed as KitParsed } from "@roost/grid";

import type { Value } from "../../ipc";
import { rawText } from "../../lib/value";
import { prettyXml } from "../../lib/xml";
import type { GridColumn } from "../grid/DataGrid";

/** `long`: JSON, XML and long or multi-line text, edited in the cell viewer panel. */
export type { EditorKind } from "@roost/grid";

const LONG_TEXT = 200;
const INTEGER = /^[-+]?\d+$/;
const DECIMAL = /^[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$/;
const DATE = /^\d{4}-\d{2}-\d{2}$/;
const DATE_TIME = /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?([+-]\d{2}(:?\d{2})?|Z)?$/;

export function editorKind(column: GridColumn, value: Value): EditorKind {
  if (value.t === "bytes" || column.category === "binary") return "binary";
  if (
    value.t === "array" ||
    value.t === "unknown" ||
    column.category === "array" ||
    column.category === "other"
  ) {
    return "none";
  }
  if (value.t === "bool" || column.category === "bool") return "bool";
  if (
    value.t === "json" ||
    value.t === "xml" ||
    column.category === "json" ||
    column.category === "xml"
  )
    return "long";
  if (
    value.t === "text" &&
    (value.v.length > LONG_TEXT || /[\r\n]/.test(value.v) || value.truncated === true)
  ) {
    return "long";
  }
  if (["int", "uint", "float", "decimal"].includes(value.t)) return "number";
  if (["integer", "float", "decimal"].includes(column.category)) return "number";
  if (
    ["date", "time", "timestamp", "timestamptz", "interval"].includes(value.t) ||
    column.category === "temporal"
  ) {
    return "temporal";
  }
  return "text";
}

type TemporalTag = "date" | "time" | "timestamp" | "timestamptz" | "interval";

function temporalTag(column: GridColumn, original: Value): TemporalTag {
  if (
    original.t === "date" ||
    original.t === "time" ||
    original.t === "timestamp" ||
    original.t === "timestamptz" ||
    original.t === "interval"
  ) {
    return original.t;
  }
  const type = column.typeName.toLowerCase();
  if (type.includes("interval")) return "interval";
  if (type.includes("timestamptz") || type.includes("with time zone")) return "timestamptz";
  if (type.includes("timestamp") || type.includes("datetime")) return "timestamp";
  if (type.includes("time")) return "time";
  return "date";
}

export function formatHint(column: GridColumn, original: Value): string {
  switch (temporalTag(column, original)) {
    case "date":
      return "YYYY-MM-DD";
    case "time":
      return "HH:MM:SS";
    case "timestamp":
      return "YYYY-MM-DD HH:MM:SS";
    case "timestamptz":
      return "YYYY-MM-DD HH:MM:SS+HH:MM";
    case "interval":
      return "e.g. 1 day 02:00:00";
  }
}

/** The editor's starting text: the whole value, or nothing for `NULL`. */
export function editText(value: Value): string {
  return value.t === "null" ? "" : rawText(value);
}

export type Parsed = KitParsed<Value>;

function numberValue(text: string, column: GridColumn, original: Value): Parsed {
  const trimmed = text.trim();
  const tag =
    original.t === "int" ||
    original.t === "uint" ||
    original.t === "float" ||
    original.t === "decimal"
      ? original.t
      : null;
  const integer =
    tag === "int" || tag === "uint" || (tag === null && column.category === "integer");
  if (integer) {
    if (!INTEGER.test(trimmed)) return { error: "Enter a whole number" };
    const unsigned = tag === "uint" || column.typeName.toLowerCase().includes("unsigned");
    if (unsigned && trimmed.startsWith("-")) return { error: "Enter a number of 0 or more" };
    return { value: { t: unsigned ? "uint" : "int", v: trimmed.replace(/^\+/, "") } };
  }
  if (!DECIMAL.test(trimmed)) return { error: "Enter a number" };
  const float = tag === "float" || (tag === null && column.category === "float");
  return { value: { t: float ? "float" : "decimal", v: trimmed } };
}

function temporalValue(text: string, column: GridColumn, original: Value): Parsed {
  const trimmed = text.trim();
  const tag = temporalTag(column, original);
  if (tag === "date" && !DATE.test(trimmed))
    return { error: `Expected ${formatHint(column, original)}` };
  if ((tag === "timestamp" || tag === "timestamptz") && !DATE_TIME.test(trimmed)) {
    return { error: `Expected ${formatHint(column, original)}` };
  }
  const iso = tag === "timestamp" || tag === "timestamptz" ? trimmed.replace(" ", "T") : trimmed;
  return { value: { t: tag, v: iso } };
}

function longValue(text: string, column: GridColumn, original: Value): Parsed {
  const json = original.t === "json" || (original.t === "null" && column.category === "json");
  const xml = original.t === "xml" || (original.t === "null" && column.category === "xml");
  if (json) {
    try {
      JSON.parse(text);
    } catch {
      return { error: "Not valid JSON" };
    }
    return { value: { t: "json", v: text } };
  }
  if (xml) {
    if (prettyXml(text) === null) return { error: "Not well-formed XML" };
    return { value: { t: "xml", v: text } };
  }
  return { value: { t: "text", v: text } };
}

/** The value an editor's text means for this column. An empty text cell is an empty string,
 * never `NULL`: that is set explicitly. */
export function parseInput(text: string, column: GridColumn, original: Value): Parsed {
  switch (editorKind(column, original)) {
    case "number":
      return numberValue(text, column, original);
    case "temporal":
      return temporalValue(text, column, original);
    case "long":
      return longValue(text, column, original);
    case "bool": {
      const lower = text.trim().toLowerCase();
      if (["true", "1", "t", "yes"].includes(lower)) return { value: { t: "bool", v: true } };
      if (["false", "0", "f", "no"].includes(lower)) return { value: { t: "bool", v: false } };
      return { error: "Enter true or false" };
    }
    case "binary":
    case "none":
      return { error: "This value can only be set to NULL" };
    case "text":
      return { value: original.t === "uuid" ? { t: "uuid", v: text } : { t: "text", v: text } };
  }
}

/** The next value of a boolean cell: `NULL` and `false` become `true`, `true` becomes `false`. */
export function toggled(value: Value): Value {
  return { t: "bool", v: !(value.t === "bool" && value.v) };
}
