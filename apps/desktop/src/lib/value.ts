/** Display formatting for database values. Values are shown as text, never as HTML. */
import type { TypeCategory, Value } from "../ipc";

export type FormattedValue = {
  text: string;
  isNull: boolean;
  truncated: boolean;
};

const NULL_TEXT = "NULL";
const ELLIPSIS = "…";
/** Bytes shown in a grid cell before the preview is cut. */
const BYTES_PREVIEW = 16;
const BYTES_PER_KIB = 1024;

export function formatValue(value: Value): FormattedValue {
  switch (value.t) {
    case "null":
      return { text: NULL_TEXT, isNull: true, truncated: false };
    case "bool":
      return plain(String(value.v));
    case "text":
    case "json":
    case "xml":
      return {
        text: value.truncated ? value.v + ELLIPSIS : value.v,
        isNull: false,
        truncated: value.truncated ?? false,
      };
    case "bytes":
      return formatBytes(value.v, value.len, value.truncated ?? false);
    case "array":
      return plain(`[${value.v.map((item) => formatValue(item).text).join(", ")}]`);
    case "unknown":
      return plain(value.text ?? (value.hex ? `0x${value.hex}` : `<${value.type}>`));
    default:
      return plain(value.v);
  }
}

export function isNumericCategory(category: TypeCategory): boolean {
  return category === "integer" || category === "float" || category === "decimal";
}

function plain(text: string): FormattedValue {
  return { text, isNull: false, truncated: false };
}

function formatBytes(base64: string, length: number, truncated: boolean): FormattedValue {
  const bytes = decodeBase64(base64);
  const shown = bytes.slice(0, BYTES_PREVIEW);
  const hex = Array.from(shown, (b) => b.toString(16).padStart(2, "0")).join("");
  const cut = bytes.length > BYTES_PREVIEW || truncated ? ELLIPSIS : "";
  return { text: `0x${hex}${cut} (${formatSize(length)})`, isNull: false, truncated };
}

function decodeBase64(base64: string): Uint8Array {
  const binary = atob(base64);
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

export function formatSize(bytes: number): string {
  if (bytes < BYTES_PER_KIB) return `${bytes} B`;
  const kib = bytes / BYTES_PER_KIB;
  if (kib < BYTES_PER_KIB) return `${kib.toFixed(1)} KiB`;
  return `${(kib / BYTES_PER_KIB).toFixed(1)} MiB`;
}
