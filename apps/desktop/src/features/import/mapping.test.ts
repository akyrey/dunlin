import { describe, expect, it } from "vitest";

import type { Column } from "../../ipc";
import { autoMap, delimiterFor, importColumns } from "./mapping";

const column = (name: string, generated = false): Column => ({
  name,
  typeName: "text",
  category: "text",
  nullable: true,
  default: null,
  comment: null,
  ordinal: 1,
  generated,
});

describe("autoMap", () => {
  const columns = [column("id"), column("full_name"), column("total", true), column("email")];

  it("matches headers to columns by name, ignoring case, spaces and underscores", () => {
    expect(autoMap(["Email", "Full Name", "ID", "extra"], columns, true)).toEqual({
      id: 2,
      full_name: 1,
      email: 0,
    });
  });

  it("maps by position without a header, skipping generated columns", () => {
    expect(autoMap(["Column 1", "Column 2", "Column 3"], columns, false)).toEqual({
      id: 0,
      full_name: 1,
      email: 2,
    });
  });
});

describe("importColumns", () => {
  it("lists the mapped columns and leaves the rest to their defaults", () => {
    expect(importColumns({ id: 2, email: null, name: 0 })).toEqual([
      { column: "id", source: 2 },
      { column: "name", source: 0 },
    ]);
  });
});

describe("delimiterFor", () => {
  it("guesses tab for .tsv files and comma otherwise", () => {
    expect(delimiterFor("/data/people.TSV")).toBe("\t");
    expect(delimiterFor("/data/people.csv")).toBe(",");
  });
});
