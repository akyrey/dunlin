/** The Columns sub-tab in edit mode (task 9.11): every column's name, type, nullability,
 * default and comment as inputs, with drop, restore and add. */
import { Button } from "@roost/ui";
import { useId } from "react";

import type { Column, ColumnDefinition, TableInfo } from "../../ipc";
import type { SchemaAction, SchemaPending } from "./schemaPending";
import { definitionOf } from "./schemaPending";

/** Suggested in the type field; any type the database knows can be typed. */
const COMMON_TYPES = [
  "integer",
  "bigint",
  "smallint",
  "numeric(10,2)",
  "real",
  "boolean",
  "text",
  "varchar(255)",
  "date",
  "timestamp",
  "json",
];

type Props = {
  info: TableInfo;
  pending: SchemaPending;
  dispatch: (action: SchemaAction) => void;
};

const input =
  "w-full rounded border border-zinc-700 bg-zinc-950 px-1.5 py-0.5 font-mono text-xs text-zinc-100 disabled:opacity-50";

let nextTempId = 0;

export function EditableColumns({ info, pending, dispatch }: Props) {
  const typesId = useId();
  const types = [...new Set([...info.columns.map((c) => c.typeName), ...COMMON_TYPES])];
  return (
    <div className="flex flex-col">
      <datalist id={typesId}>
        {types.map((t) => (
          <option key={t} value={t} />
        ))}
      </datalist>
      <table className="w-full text-left text-sm">
        <thead className="sticky top-0 bg-zinc-900 text-xs text-zinc-400">
          <tr>
            {["Name", "Type", "Not null", "Default", "Comment", ""].map((h) => (
              <th key={h} scope="col" className="px-2 py-1.5 font-medium">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {info.columns.map((column) => (
            <ExistingColumn
              key={column.name}
              column={column}
              pending={pending}
              dispatch={dispatch}
              typesId={typesId}
            />
          ))}
          {pending.added.map(({ tempId, column }) => (
            <ColumnRow
              key={tempId}
              label={`New column ${column.name || "(unnamed)"}`}
              state="added"
              definition={column}
              typesId={typesId}
              onChange={(next) => {
                dispatch({ type: "editAdded", tempId, column: next });
              }}
              action={
                <Button
                  variant="ghost"
                  className="py-0 text-xs"
                  onClick={() => {
                    dispatch({ type: "removeAdded", tempId });
                  }}
                >
                  Remove
                </Button>
              }
            />
          ))}
        </tbody>
      </table>
      <div className="px-2 py-2">
        <Button
          variant="ghost"
          className="text-xs"
          onClick={() => {
            nextTempId += 1;
            dispatch({
              type: "addColumn",
              tempId: `new-${nextTempId}`,
              column: { name: "", typeName: "", nullable: true, default: null, comment: null },
            });
          }}
        >
          Add column
        </Button>
      </div>
    </div>
  );
}

function ExistingColumn({
  column,
  pending,
  dispatch,
  typesId,
}: {
  column: Column;
  pending: SchemaPending;
  dispatch: (action: SchemaAction) => void;
  typesId: string;
}) {
  const dropped = pending.droppedColumns.includes(column.name);
  const altered = pending.altered[column.name];
  const definition = altered?.column ?? definitionOf(column);
  const state = dropped ? "dropped" : altered ? "changed" : "unchanged";
  return (
    <ColumnRow
      label={`Column ${column.name}`}
      state={state}
      definition={definition}
      typesId={typesId}
      locked={
        column.generated ? "Generated columns are changed by dropping and re-adding them" : null
      }
      onChange={(next) => {
        dispatch({ type: "alterColumn", name: column.name, column: next, using: altered?.using });
      }}
      action={
        dropped ? (
          <Button
            variant="ghost"
            className="py-0 text-xs"
            onClick={() => {
              dispatch({ type: "restoreColumn", name: column.name });
            }}
          >
            Restore
          </Button>
        ) : (
          <Button
            variant="ghost"
            className="py-0 text-xs text-rose-300"
            onClick={() => {
              dispatch({ type: "dropColumn", name: column.name });
            }}
          >
            Drop
          </Button>
        )
      }
    />
  );
}

const ROW_STATE: Record<string, string> = {
  unchanged: "",
  changed: "bg-sky-950/40",
  added: "bg-emerald-950/40",
  dropped: "bg-rose-950/40 line-through opacity-60",
};

function ColumnRow({
  label,
  state,
  definition,
  typesId,
  locked = null,
  onChange,
  action,
}: {
  label: string;
  state: "unchanged" | "changed" | "added" | "dropped";
  definition: ColumnDefinition;
  typesId: string;
  locked?: string | null;
  onChange: (next: ColumnDefinition) => void;
  action: React.ReactNode;
}) {
  const disabled = state === "dropped" || locked !== null;
  const text = (value: string | null) => value ?? "";
  const orNull = (value: string) => (value.trim() === "" ? null : value);
  return (
    <tr
      aria-label={label}
      data-state={state}
      title={locked ?? undefined}
      className={`border-t border-zinc-800/70 ${ROW_STATE[state]}`}
    >
      <td className="px-2 py-1">
        <input
          aria-label="Name"
          className={input}
          disabled={disabled}
          value={definition.name}
          onChange={(e) => {
            onChange({ ...definition, name: e.target.value });
          }}
        />
      </td>
      <td className="px-2 py-1">
        <input
          aria-label="Type"
          className={input}
          list={typesId}
          disabled={disabled}
          value={definition.typeName}
          onChange={(e) => {
            onChange({ ...definition, typeName: e.target.value });
          }}
        />
      </td>
      <td className="px-2 py-1 text-center">
        <input
          aria-label="Not null"
          type="checkbox"
          disabled={disabled}
          checked={!definition.nullable}
          onChange={(e) => {
            onChange({ ...definition, nullable: !e.target.checked });
          }}
        />
      </td>
      <td className="px-2 py-1">
        <input
          aria-label="Default"
          className={input}
          placeholder="none"
          disabled={disabled}
          value={text(definition.default)}
          onChange={(e) => {
            onChange({ ...definition, default: orNull(e.target.value) });
          }}
        />
      </td>
      <td className="px-2 py-1">
        <input
          aria-label="Comment"
          className={`${input} font-sans`}
          disabled={disabled}
          value={text(definition.comment)}
          onChange={(e) => {
            onChange({ ...definition, comment: orNull(e.target.value) });
          }}
        />
      </td>
      <td className="px-2 py-1 text-right">{action}</td>
    </tr>
  );
}
