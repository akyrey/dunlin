/** Staged schema changes of one table detail tab (task 9.10, D-042). A pure reducer over
 * immutable state: nothing reaches the database before Apply. `toChanges` writes the list the
 * backend validates, in the order it expects: drops of keys, constraints and indexes, then column
 * changes, then additions, then the comment and the table rename. */
import type {
  Check,
  Column,
  ColumnDefinition,
  ForeignKey,
  Key,
  SchemaChange,
  TableIndex,
  TableInfo,
} from "../../ipc";

/** A key, constraint or index, as the detail tab lists them. */
export type SchemaObject =
  | { kind: "primaryKey"; value: Key }
  | { kind: "unique"; value: Key }
  | { kind: "foreignKey"; value: ForeignKey }
  | { kind: "check"; value: Check }
  | { kind: "index"; value: TableIndex };

export type AlteredColumn = { column: ColumnDefinition; using: string | null };

export type SchemaPending = {
  /** By the column's original name. */
  altered: Record<string, AlteredColumn>;
  droppedColumns: string[];
  added: { tempId: string; column: ColumnDefinition }[];
  droppedObjects: SchemaObject[];
  addedObjects: SchemaObject[];
  /** `undefined`: unchanged. */
  comment?: string | null;
  tableName?: string;
};

export const EMPTY: SchemaPending = {
  altered: {},
  droppedColumns: [],
  added: [],
  droppedObjects: [],
  addedObjects: [],
};

export type SchemaAction =
  | { type: "alterColumn"; name: string; column: ColumnDefinition; using?: string | null }
  | { type: "dropColumn"; name: string }
  | { type: "restoreColumn"; name: string }
  | { type: "addColumn"; tempId: string; column: ColumnDefinition }
  | { type: "editAdded"; tempId: string; column: ColumnDefinition }
  | { type: "removeAdded"; tempId: string }
  | { type: "dropObject"; object: SchemaObject }
  | { type: "restoreObject"; object: SchemaObject }
  | { type: "addObject"; object: SchemaObject }
  /** Index into `addedObjects`. */
  | { type: "removeObject"; index: number }
  | { type: "setComment"; comment: string | null }
  | { type: "renameTable"; name: string }
  | { type: "discard" };

export function definitionOf(column: Column): ColumnDefinition {
  return {
    name: column.name,
    typeName: column.typeName,
    nullable: column.nullable,
    default: column.default,
    comment: column.comment,
  };
}

const same = (a: unknown, b: unknown): boolean => JSON.stringify(a) === JSON.stringify(b);

function omit(state: SchemaPending, key: "comment" | "tableName"): SchemaPending {
  return Object.fromEntries(Object.entries(state).filter(([k]) => k !== key)) as SchemaPending;
}

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([k]) => k !== key));
}

/** `original` is the table the changes are made against: an alter, comment or name back to
 * what it was is no change. */
export function reduce(
  state: SchemaPending,
  action: SchemaAction,
  original: TableInfo,
): SchemaPending {
  switch (action.type) {
    case "alterColumn": {
      const before = original.columns.find((c) => c.name === action.name);
      const using = action.using ?? null;
      if (before && same(definitionOf(before), action.column) && using === null) {
        return { ...state, altered: without(state.altered, action.name) };
      }
      return {
        ...state,
        altered: { ...state.altered, [action.name]: { column: action.column, using } },
      };
    }
    case "dropColumn":
      if (state.droppedColumns.includes(action.name)) return state;
      return {
        ...state,
        altered: without(state.altered, action.name),
        droppedColumns: [...state.droppedColumns, action.name],
      };
    case "restoreColumn":
      return { ...state, droppedColumns: state.droppedColumns.filter((n) => n !== action.name) };
    case "addColumn":
      return {
        ...state,
        added: [...state.added, { tempId: action.tempId, column: action.column }],
      };
    case "editAdded":
      return {
        ...state,
        added: state.added.map((a) =>
          a.tempId === action.tempId ? { ...a, column: action.column } : a,
        ),
      };
    case "removeAdded":
      return { ...state, added: state.added.filter((a) => a.tempId !== action.tempId) };
    case "dropObject":
      if (state.droppedObjects.some((o) => same(o, action.object))) return state;
      return { ...state, droppedObjects: [...state.droppedObjects, action.object] };
    case "restoreObject":
      return {
        ...state,
        droppedObjects: state.droppedObjects.filter((o) => !same(o, action.object)),
      };
    case "addObject":
      return { ...state, addedObjects: [...state.addedObjects, action.object] };
    case "removeObject":
      return { ...state, addedObjects: state.addedObjects.filter((_, i) => i !== action.index) };
    case "setComment":
      return setComment(state, action.comment, original);
    case "renameTable":
      return renameTable(state, action.name, original);
    case "discard":
      return EMPTY;
  }
}

function setComment(
  state: SchemaPending,
  comment: string | null,
  original: TableInfo,
): SchemaPending {
  const value = comment?.trim() ? comment : null;
  const rest = omit(state, "comment");
  return value === original.comment ? rest : { ...rest, comment: value };
}

function renameTable(state: SchemaPending, name: string, original: TableInfo): SchemaPending {
  const rest = omit(state, "tableName");
  return name === original.table.name || name.trim() === "" ? rest : { ...rest, tableName: name };
}

export function changeCount(state: SchemaPending): number {
  return (
    Object.keys(state.altered).length +
    state.droppedColumns.length +
    state.added.length +
    state.droppedObjects.length +
    state.addedObjects.length +
    (state.comment === undefined ? 0 : 1) +
    (state.tableName === undefined ? 0 : 1)
  );
}

function dropChange(object: SchemaObject): SchemaChange {
  switch (object.kind) {
    case "primaryKey":
      return { kind: "drop_primary_key" };
    case "unique":
      return { kind: "drop_unique", key: object.value };
    case "foreignKey":
      return { kind: "drop_foreign_key", foreignKey: object.value };
    case "check":
      return { kind: "drop_check", check: object.value };
    case "index":
      return { kind: "drop_index", index: object.value };
  }
}

function addChange(object: SchemaObject): SchemaChange {
  switch (object.kind) {
    case "primaryKey":
      return { kind: "add_primary_key", key: object.value };
    case "unique":
      return { kind: "add_unique", key: object.value };
    case "foreignKey":
      return { kind: "add_foreign_key", foreignKey: object.value };
    case "check":
      return { kind: "add_check", check: object.value };
    case "index":
      return { kind: "add_index", index: object.value };
  }
}

/** Foreign keys are dropped before the indexes that may back them (MySQL), and indexes are
 * added before the foreign keys that may use them. */
const DROP_ORDER: SchemaObject["kind"][] = ["foreignKey", "primaryKey", "unique", "check", "index"];
const ADD_ORDER: SchemaObject["kind"][] = ["primaryKey", "unique", "check", "index", "foreignKey"];

function ordered(objects: SchemaObject[], order: SchemaObject["kind"][]): SchemaObject[] {
  return [...objects].sort((a, b) => order.indexOf(a.kind) - order.indexOf(b.kind));
}

/** The changes to send, in the order the backend applies and validates them. */
export function toChanges(info: TableInfo, state: SchemaPending): SchemaChange[] {
  const alters = info.columns.flatMap((c): SchemaChange[] => {
    const altered = state.altered[c.name];
    if (!altered || state.droppedColumns.includes(c.name)) return [];
    return [{ kind: "alter_column", name: c.name, column: altered.column, using: altered.using }];
  });
  return [
    ...ordered(state.droppedObjects, DROP_ORDER).map(dropChange),
    ...state.droppedColumns.map((name): SchemaChange => ({ kind: "drop_column", name })),
    ...alters,
    ...state.added.map(({ column }): SchemaChange => ({ kind: "add_column", column })),
    ...ordered(state.addedObjects, ADD_ORDER).map(addChange),
    ...(state.comment === undefined
      ? []
      : [{ kind: "set_comment", comment: state.comment } satisfies SchemaChange]),
    ...(state.tableName === undefined
      ? []
      : [{ kind: "rename_table", name: state.tableName } satisfies SchemaChange]),
  ];
}
