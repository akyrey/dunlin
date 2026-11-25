/** The form for a new key, constraint or index, and the object it describes (task 9.11). */
import type { ReferentialAction, TableInfo } from "../../ipc";
import type { SchemaObject } from "./schemaPending";

/** `a, lower(b), c` → three parts: commas inside parentheses do not split. */
export function splitList(text: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const ch of text) {
    if (ch === "(") depth += 1;
    if (ch === ")") depth = Math.max(0, depth - 1);
    if (ch === "," && depth === 0) {
      parts.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  parts.push(current);
  return parts.map((p) => p.trim()).filter((p) => p !== "");
}

export type Draft = {
  kind: SchemaObject["kind"];
  name: string;
  columns: string;
  expression: string;
  unique: boolean;
  method: string;
  table: string;
  referencedColumns: string;
  onDelete: ReferentialAction | "";
  onUpdate: ReferentialAction | "";
};

export const blank = (kind: SchemaObject["kind"]): Draft => ({
  kind,
  name: "",
  columns: "",
  expression: "",
  unique: false,
  method: "",
  table: "",
  referencedColumns: "",
  onDelete: "",
  onUpdate: "",
});

const orNull = (text: string) => (text.trim() === "" ? null : text.trim());

/** The object a draft describes, or why it is not complete. */
export function objectOf(draft: Draft, info: TableInfo): SchemaObject | string {
  const columns = splitList(draft.columns);
  const name = orNull(draft.name);
  switch (draft.kind) {
    case "check":
      return draft.expression.trim()
        ? { kind: "check", value: { name, expression: draft.expression.trim() } }
        : "Write the check expression";
    case "index":
      if (!name) return "Name the index";
      if (columns.length === 0) return "List the indexed columns or expressions";
      return {
        kind: "index",
        value: {
          name,
          columns,
          unique: draft.unique,
          method: orNull(draft.method),
          predicate: orNull(draft.expression),
        },
      };
    case "foreignKey": {
      const referenced = splitList(draft.referencedColumns);
      if (columns.length === 0 || !draft.table.trim()) return "Name the columns and the table";
      if (referenced.length !== columns.length) return "List as many referenced columns as columns";
      const dot = draft.table.indexOf(".");
      const referencedTable =
        dot > 0
          ? {
              namespace: { ...info.table.namespace, schema: draft.table.slice(0, dot).trim() },
              name: draft.table.slice(dot + 1).trim(),
            }
          : { namespace: info.table.namespace, name: draft.table.trim() };
      return {
        kind: "foreignKey",
        value: {
          name,
          columns,
          referencedTable,
          referencedColumns: referenced,
          onDelete: draft.onDelete || null,
          onUpdate: draft.onUpdate || null,
        },
      };
    }
    default:
      return columns.length > 0
        ? { kind: draft.kind, value: { name, columns } }
        : "List the key's columns";
  }
}
