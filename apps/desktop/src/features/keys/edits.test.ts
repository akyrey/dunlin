import { describe, expect, it } from "vitest";

import type { Value } from "../../ipc";
import { ACTIONS, buildEdit, keyIdOf, prefill, ttlEdit } from "./edits";

const text = (v: string): Value => ({ t: "text", v });

describe("keyIdOf", () => {
  it("encodes the UTF-8 bytes of a name as base64", () => {
    expect(keyIdOf("user:1")).toBe(btoa("user:1"));
    expect(keyIdOf("é")).toBe("w6k=");
  });
});

describe("ttlEdit", () => {
  it("turns seconds into milliseconds", () => {
    expect(ttlEdit("90")).toEqual({ kind: "expire", ttl_ms: 90_000 });
    expect(ttlEdit(" 1.5 ")).toEqual({ kind: "expire", ttl_ms: 1500 });
  });

  it("clears the TTL when empty", () => {
    expect(ttlEdit("")).toEqual({ kind: "expire", ttl_ms: null });
  });

  it("refuses anything but a positive number", () => {
    expect(ttlEdit("0")).toBe("The TTL must be a positive number of seconds.");
    expect(ttlEdit("abc")).toBe("The TTL must be a positive number of seconds.");
    expect(ttlEdit("-3")).toBe("The TTL must be a positive number of seconds.");
  });
});

describe("buildEdit", () => {
  it("offers actions per type and none for read-only types", () => {
    expect(ACTIONS.hash?.map((a) => a.label)).toEqual(["Set field", "Delete field"]);
    expect(ACTIONS.stream).toBeUndefined();
  });

  it("builds hash, list, set and sorted-set edits", () => {
    expect(buildEdit("hash_set", { field: "name", value: "Ada" })).toEqual({
      kind: "hash_set",
      field: "name",
      value: "Ada",
    });
    expect(buildEdit("list_set", { index: "-1", value: "x" })).toEqual({
      kind: "list_set",
      index: -1,
      value: "x",
    });
    expect(buildEdit("list_push_head", { value: "x" })).toEqual({
      kind: "list_push",
      value: "x",
      head: true,
    });
    expect(buildEdit("set_remove", { member: "m" })).toEqual({ kind: "set_remove", member: "m" });
    expect(buildEdit("zset_add", { member: "m", score: "2.5" })).toEqual({
      kind: "zset_add",
      member: "m",
      score: 2.5,
    });
  });

  it("refuses a bad index or score", () => {
    expect(buildEdit("list_set", { index: "1.5", value: "x" })).toBe(
      "The index must be a whole number.",
    );
    expect(buildEdit("zset_add", { member: "m", score: "" })).toBe("The score must be a number.");
    expect(buildEdit("zset_add", { member: "m", score: "inf" })).toBe(
      "The score must be a number.",
    );
  });

  it("keeps empty values, which Redis allows", () => {
    expect(buildEdit("set_string", {})).toEqual({ kind: "set_string", value: "" });
  });
});

describe("prefill", () => {
  it("maps the selected row onto the form's fields by column name", () => {
    const columns = ["field", "value"];
    expect(prefill(columns, [text("name"), text("Ada")])).toEqual({ field: "name", value: "Ada" });
  });

  it("maps a sorted set's score and a list's index", () => {
    expect(prefill(["member", "score"], [text("m"), { t: "float", v: "2.5" }])).toEqual({
      member: "m",
      score: "2.5",
    });
    expect(prefill(["index", "value"], [{ t: "int", v: "3" }, text("x")])).toEqual({
      index: "3",
      value: "x",
    });
  });

  it("leaves out cut values, so a save never writes a shortened value back", () => {
    const cut: Value = { t: "text", v: "Ad", truncated: true };
    expect(prefill(["field", "value"], [text("name"), cut])).toEqual({ field: "name" });
  });
});
