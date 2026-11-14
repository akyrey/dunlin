/** Rows as JSON for the JSON view (task 6.9). Numbers stay numbers only when a JavaScript
 * number holds them exactly; otherwise they are strings, so no digit is ever lost. */
import type { Value } from "../ipc";

const SAFE_INTEGER = /^-?\d+$/;

/** `1.50` and `1.5` are the same number; compare without trailing zeros or a leading `+`. */
function canonical(text: string): string {
  const unsigned = text.replace(/^\+/, "");
  if (!unsigned.includes(".")) return unsigned;
  return unsigned.replace(/0+$/, "").replace(/\.$/, "");
}

/** The number, when converting `text` to a double and back gives the same value. */
export function exactNumber(text: string): number | null {
  const number = Number(text);
  if (!Number.isFinite(number)) return null;
  if (SAFE_INTEGER.test(text)) return Number.isSafeInteger(number) ? number : null;
  return canonical(String(number)) === canonical(text) ? number : null;
}

export function toJsonValue(value: Value): unknown {
  switch (value.t) {
    case "null":
      return null;
    case "bool":
      return value.v;
    case "int":
    case "uint":
    case "float":
    case "decimal":
      return exactNumber(value.v) ?? value.v;
    case "bytes":
      return value.v;
    case "json":
      if (!value.truncated) {
        try {
          return JSON.parse(value.v) as unknown;
        } catch {
          // Not valid JSON after all: keep the text.
        }
      }
      return value.v;
    case "text":
    case "xml":
      return value.v;
    case "array":
      return value.v.map(toJsonValue);
    case "unknown":
      return value.text ?? value.hex ?? null;
    default:
      return value.v;
  }
}

export function rowsToJson(columns: string[], rows: Value[][]): string {
  const objects = rows.map((row) =>
    Object.fromEntries(columns.map((name, i) => [name, toJsonValue(row[i] ?? { t: "null" })])),
  );
  return JSON.stringify(objects, null, 2);
}

/** Pretty-printed JSON, or `null` when the text does not parse. */
export function prettyJson(text: string): string | null {
  try {
    return JSON.stringify(JSON.parse(text) as unknown, null, 2);
  } catch {
    return null;
  }
}
