import type { KeyEdit, Value } from "../../ipc";
import { rawText } from "../../lib/value";

/** The inputs of the value editor; a column of the key's grid fills the input of its name. */
export type Field = "value" | "field" | "index" | "member" | "score";
export type Inputs = Partial<Record<Field, string>>;

export type ActionId =
  | "set_string"
  | "hash_set"
  | "hash_delete"
  | "list_set"
  | "list_push_head"
  | "list_push_tail"
  | "list_remove"
  | "set_add"
  | "set_remove"
  | "zset_add"
  | "zset_remove";

export type Action = { id: ActionId; label: string; fields: Field[]; removes?: boolean };

/** What the value editor offers per key type; types without an entry are read-only (D-032). */
export const ACTIONS: Partial<Record<string, Action[]>> = {
  string: [{ id: "set_string", label: "Save", fields: ["value"] }],
  hash: [
    { id: "hash_set", label: "Set field", fields: ["field", "value"] },
    { id: "hash_delete", label: "Delete field", fields: ["field"], removes: true },
  ],
  list: [
    { id: "list_set", label: "Set at index", fields: ["index", "value"] },
    { id: "list_push_head", label: "Push to head", fields: ["value"] },
    { id: "list_push_tail", label: "Push to tail", fields: ["value"] },
    { id: "list_remove", label: "Remove value", fields: ["value"], removes: true },
  ],
  set: [
    { id: "set_add", label: "Add member", fields: ["member"] },
    { id: "set_remove", label: "Remove member", fields: ["member"], removes: true },
  ],
  zset: [
    { id: "zset_add", label: "Add or rescore", fields: ["member", "score"] },
    { id: "zset_remove", label: "Remove member", fields: ["member"], removes: true },
  ],
};

/** The types a new key can have, and the write that creates each with its first element. */
export const CREATE_ACTIONS: Record<string, ActionId> = {
  string: "set_string",
  hash: "hash_set",
  list: "list_push_tail",
  set: "set_add",
  zset: "zset_add",
};

/** The fields a type's editor shows, in order. */
export function fieldsOf(keyType: string): Field[] {
  const fields = (ACTIONS[keyType] ?? []).flatMap((action) => action.fields);
  return [...new Set(fields)];
}

/** A key's id from a typed name: base64 of its UTF-8 bytes, as the backend expects. */
export function keyIdOf(name: string): string {
  const bytes = new TextEncoder().encode(name);
  return btoa(String.fromCharCode(...bytes));
}

const MS_PER_SECOND = 1000;

/** An expiry from seconds; empty clears the TTL. A message when the input is not valid. */
export function ttlEdit(seconds: string): KeyEdit | string {
  const trimmed = seconds.trim();
  if (trimmed === "") return { kind: "expire", ttl_ms: null };
  const parsed = Number(trimmed);
  const ttlMs = Math.round(parsed * MS_PER_SECOND);
  if (!Number.isFinite(parsed) || ttlMs <= 0) {
    return "The TTL must be a positive number of seconds.";
  }
  return { kind: "expire", ttl_ms: ttlMs };
}

function score(input: string | undefined): number | string {
  const parsed = Number(input?.trim() || "x");
  return Number.isFinite(parsed) ? parsed : "The score must be a number.";
}

function index(input: string | undefined): number | string {
  const parsed = Number(input?.trim() || "x");
  return Number.isSafeInteger(parsed) ? parsed : "The index must be a whole number.";
}

/** The edit an action makes from the inputs, or a message when they are not valid. */
export function buildEdit(action: ActionId, inputs: Inputs): KeyEdit | string {
  const { value = "", field = "", member = "" } = inputs;
  switch (action) {
    case "set_string":
      return { kind: "set_string", value };
    case "hash_set":
      return { kind: "hash_set", field, value };
    case "hash_delete":
      return { kind: "hash_delete", field };
    case "list_set": {
      const at = index(inputs.index);
      return typeof at === "string" ? at : { kind: "list_set", index: at, value };
    }
    case "list_push_head":
    case "list_push_tail":
      return { kind: "list_push", value, head: action === "list_push_head" };
    case "list_remove":
      return { kind: "list_remove", value };
    case "set_add":
      return { kind: "set_add", member };
    case "set_remove":
      return { kind: "set_remove", member };
    case "zset_add": {
      const parsed = score(inputs.score);
      return typeof parsed === "string" ? parsed : { kind: "zset_add", member, score: parsed };
    }
    case "zset_remove":
      return { kind: "zset_remove", member };
  }
}

const FIELDS: ReadonlySet<string> = new Set<Field>(["value", "field", "index", "member", "score"]);

function isCut(value: Value): boolean {
  return "truncated" in value && value.truncated === true;
}

/** The inputs a selected grid row fills; cut values are left out so they are never saved back. */
export function prefill(columns: string[], row: Value[]): Inputs {
  return Object.fromEntries(
    columns.flatMap((column, i) => {
      const cell = row[i];
      if (!FIELDS.has(column) || cell === undefined || isCut(cell)) return [];
      return [[column, rawText(cell)]];
    }),
  );
}
