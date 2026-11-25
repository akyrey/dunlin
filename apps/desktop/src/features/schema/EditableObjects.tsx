/** Indexes, keys and constraints in edit mode (task 9.11): drop and restore what exists, add new
 * ones. Changing one is dropping it and adding the new version. */
import { Button } from "@roost/ui";
import { type ReactNode, useState } from "react";

import type { ReferentialAction, TableInfo } from "../../ipc";
import { type Draft, blank, objectOf } from "./objectDraft";
import type { SchemaAction, SchemaObject, SchemaPending } from "./schemaPending";

type Props = {
  info: TableInfo;
  pending: SchemaPending;
  dispatch: (action: SchemaAction) => void;
};

const same = (a: SchemaObject, b: SchemaObject) => JSON.stringify(a) === JSON.stringify(b);

const ACTIONS: { value: ReferentialAction | ""; label: string }[] = [
  { value: "", label: "NO ACTION" },
  { value: "cascade", label: "CASCADE" },
  { value: "set_null", label: "SET NULL" },
  { value: "set_default", label: "SET DEFAULT" },
  { value: "restrict", label: "RESTRICT" },
];

const KIND_LABEL: Record<SchemaObject["kind"], string> = {
  primaryKey: "Primary key",
  unique: "Unique",
  foreignKey: "Foreign key",
  check: "Check",
  index: "Index",
};

function describe(object: SchemaObject): { name: string; definition: string } {
  switch (object.kind) {
    case "primaryKey":
    case "unique":
      return { name: object.value.name ?? "", definition: object.value.columns.join(", ") };
    case "check":
      return { name: object.value.name ?? "", definition: object.value.expression };
    case "index": {
      const i = object.value;
      const where = i.predicate ? ` WHERE ${i.predicate}` : "";
      return {
        name: i.name,
        definition: `${i.unique ? "UNIQUE " : ""}(${i.columns.join(", ")})${where}`,
      };
    }
    case "foreignKey": {
      const fk = object.value;
      const target = [fk.referencedTable.namespace.schema, fk.referencedTable.name]
        .filter(Boolean)
        .join(".");
      return {
        name: fk.name ?? "",
        definition: `(${fk.columns.join(", ")}) → ${target} (${fk.referencedColumns.join(", ")})`,
      };
    }
  }
}

function existing(info: TableInfo, kinds: SchemaObject["kind"][]): SchemaObject[] {
  const all: SchemaObject[] = [
    ...(info.primaryKey ? [{ kind: "primaryKey", value: info.primaryKey } as const] : []),
    ...info.uniqueConstraints.map((value) => ({ kind: "unique", value }) as const),
    ...info.checkConstraints.map((value) => ({ kind: "check", value }) as const),
    ...info.foreignKeys.map((value) => ({ kind: "foreignKey", value }) as const),
    ...info.indexes.map((value) => ({ kind: "index", value }) as const),
  ];
  return all.filter((o) => kinds.includes(o.kind));
}

export function EditableObjects({
  info,
  pending,
  dispatch,
  kinds,
  label,
}: Props & { kinds: SchemaObject["kind"][]; label: string }) {
  const added = pending.addedObjects
    .map((object, index) => ({ object, index }))
    .filter(({ object }) => kinds.includes(object.kind));
  return (
    <section aria-label={label} className="flex flex-col gap-2 py-2">
      <table className="w-full text-left text-sm">
        <thead className="text-xs text-zinc-400">
          <tr>
            {["Kind", "Name", "Definition", ""].map((h) => (
              <th key={h} scope="col" className="px-3 py-1.5 font-medium">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {existing(info, kinds).map((object, i) => {
            const dropped = pending.droppedObjects.some((o) => same(o, object));
            return (
              <ObjectRow
                key={`existing-${i}`}
                object={object}
                state={dropped ? "dropped" : "unchanged"}
                action={
                  <Button
                    variant="ghost"
                    className={`py-0 text-xs ${dropped ? "" : "text-rose-300"}`}
                    onClick={() => {
                      dispatch({ type: dropped ? "restoreObject" : "dropObject", object });
                    }}
                  >
                    {dropped ? "Restore" : "Drop"}
                  </Button>
                }
              />
            );
          })}
          {added.map(({ object, index }) => (
            <ObjectRow
              key={`added-${index}`}
              object={object}
              state="added"
              action={
                <Button
                  variant="ghost"
                  className="py-0 text-xs"
                  onClick={() => {
                    dispatch({ type: "removeObject", index });
                  }}
                >
                  Remove
                </Button>
              }
            />
          ))}
        </tbody>
      </table>
      <AddObject
        info={info}
        kinds={kinds}
        onAdd={(object) => {
          dispatch({ type: "addObject", object });
        }}
      />
    </section>
  );
}

const ROW_STATE = {
  unchanged: "",
  added: "bg-emerald-950/40",
  dropped: "bg-rose-950/40 line-through opacity-60",
};

function ObjectRow({
  object,
  state,
  action,
}: {
  object: SchemaObject;
  state: keyof typeof ROW_STATE;
  action: ReactNode;
}) {
  const { name, definition } = describe(object);
  return (
    <tr
      aria-label={`${KIND_LABEL[object.kind]} ${name || definition}`}
      data-state={state}
      className={`border-t border-zinc-800/70 ${ROW_STATE[state]}`}
    >
      <td className="px-3 py-1">{KIND_LABEL[object.kind]}</td>
      <td className="px-3 py-1">{name}</td>
      <td className="px-3 py-1 font-mono text-xs text-zinc-300">{definition}</td>
      <td className="px-3 py-1 text-right">{action}</td>
    </tr>
  );
}

const field =
  "rounded border border-zinc-700 bg-zinc-950 px-1.5 py-0.5 font-mono text-xs text-zinc-100";

function AddObject({
  info,
  kinds,
  onAdd,
}: {
  info: TableInfo;
  kinds: SchemaObject["kind"][];
  onAdd: (object: SchemaObject) => void;
}) {
  const [draft, setDraft] = useState<Draft | null>(null);
  if (!draft) {
    return (
      <div className="flex gap-1 px-2">
        {kinds.map((kind) => (
          <Button
            key={kind}
            variant="ghost"
            className="text-xs"
            onClick={() => {
              setDraft(blank(kind));
            }}
          >
            Add {KIND_LABEL[kind].toLowerCase()}
          </Button>
        ))}
      </div>
    );
  }
  const result = objectOf(draft, info);
  const set = (patch: Partial<Draft>) => {
    setDraft({ ...draft, ...patch });
  };
  const text = (label: string, key: keyof Draft, placeholder = "") => (
    <label className="flex flex-col gap-0.5 text-xs text-zinc-400">
      {label}
      <input
        aria-label={label}
        className={field}
        placeholder={placeholder}
        value={String(draft[key])}
        onChange={(e) => {
          set({ [key]: e.target.value });
        }}
      />
    </label>
  );
  const action = (label: string, key: "onDelete" | "onUpdate") => (
    <label className="flex flex-col gap-0.5 text-xs text-zinc-400">
      {label}
      <select
        aria-label={label}
        className={field}
        value={draft[key]}
        onChange={(e) => {
          set({ [key]: e.target.value as ReferentialAction | "" });
        }}
      >
        {ACTIONS.map((a) => (
          <option key={a.label} value={a.value}>
            {a.label}
          </option>
        ))}
      </select>
    </label>
  );
  return (
    <form
      aria-label={`New ${KIND_LABEL[draft.kind].toLowerCase()}`}
      className="mx-2 flex flex-wrap items-end gap-2 rounded border border-zinc-800 p-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (typeof result === "string") return;
        onAdd(result);
        setDraft(null);
      }}
    >
      {draft.kind !== "primaryKey" &&
        text("Name", "name", draft.kind === "index" ? "" : "optional")}
      {draft.kind !== "check" && text("Columns", "columns", "a, b")}
      {draft.kind === "check" && text("Expression", "expression", "total >= 0")}
      {draft.kind === "index" && (
        <>
          <label className="flex items-center gap-1 text-xs text-zinc-400">
            <input
              type="checkbox"
              checked={draft.unique}
              onChange={(e) => {
                set({ unique: e.target.checked });
              }}
            />
            Unique
          </label>
          {text("Method", "method", "optional")}
          {text("Where", "expression", "optional")}
        </>
      )}
      {draft.kind === "foreignKey" && (
        <>
          {text("References table", "table", "schema.table")}
          {text("Referenced columns", "referencedColumns", "id")}
          {action("On delete", "onDelete")}
          {action("On update", "onUpdate")}
        </>
      )}
      <Button
        type="submit"
        variant="primary"
        className="text-xs"
        disabled={typeof result === "string"}
      >
        Add
      </Button>
      <Button
        variant="ghost"
        className="text-xs"
        onClick={() => {
          setDraft(null);
        }}
      >
        Cancel
      </Button>
      {typeof result === "string" && <span className="text-xs text-zinc-500">{result}</span>}
    </form>
  );
}
