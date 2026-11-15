import type { Column, ImportColumn } from "../../ipc";

/** Table column name → index of the CSV field that fills it; `null` (or absent) keeps the default. */
export type Mapping = Record<string, number | null>;

const normalise = (name: string) => name.toLowerCase().replace(/[\s_-]+/g, "");

/**
 * The starting mapping: with a header, columns take the field of the same name (ignoring case,
 * spaces, `_` and `-`); without one, the writable columns take the fields in order.
 */
export function autoMap(headers: string[], columns: Column[], header: boolean): Mapping {
  const writable = columns.filter((c) => !c.generated);
  if (!header) {
    return Object.fromEntries(
      writable.slice(0, headers.length).map((column, index) => [column.name, index]),
    );
  }
  const byName = new Map(headers.map((name, index) => [normalise(name), index]));
  return Object.fromEntries(
    writable.flatMap((column) => {
      const index = byName.get(normalise(column.name));
      return index === undefined ? [] : [[column.name, index]];
    }),
  );
}

/** The mapping as the backend takes it. */
export function importColumns(mapping: Mapping): ImportColumn[] {
  return Object.entries(mapping).flatMap(([column, source]) =>
    source === null ? [] : [{ column, source }],
  );
}

/** Tab for `.tsv` files, comma for the rest. */
export function delimiterFor(path: string): string {
  return path.toLowerCase().endsWith(".tsv") ? "\t" : ",";
}
