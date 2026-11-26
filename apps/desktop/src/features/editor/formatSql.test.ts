import { describe, expect, it } from "vitest";

import { formatSql } from "./formatSql";

describe("formatSql", () => {
  it("puts clauses on their own lines, indented two spaces", () => {
    const result = formatSql("select a, b from t where x = 1", "postgres");

    expect(result).toEqual({ ok: true, text: "select\n  a,\n  b\nfrom\n  t\nwhere\n  x = 1" });
  });

  it("keeps the keyword case as written", () => {
    const result = formatSql("SELECT a FROM t", "sqlite");

    expect(result).toEqual({ ok: true, text: "SELECT\n  a\nFROM\n  t" });
  });

  it("keeps PostgreSQL placeholders and casts intact", () => {
    const result = formatSql("select $1::text", "postgres");

    expect(result).toEqual({ ok: true, text: "select\n  $1::text" });
  });

  it("formats every statement of a script", () => {
    const result = formatSql("select 1; select 2;", "mysql");

    expect(result).toEqual({ ok: true, text: "select\n  1;\n\nselect\n  2;" });
  });

  it("reports the first line of a parse error and leaves the text alone", () => {
    const result = formatSql("select (( from", "postgres");

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toMatch(/^Parse error/);
    expect(result.error).not.toContain("\n");
  });

  it("refuses MySQL scripts with DELIMITER, which the formatter would mangle", () => {
    const result = formatSql("DELIMITER //\nselect 1//\ndelimiter ;", "mysql");

    expect(result).toEqual({ ok: false, error: "Scripts with DELIMITER can't be formatted" });
  });

  it("refuses drivers that do not speak SQL", () => {
    const result = formatSql("GET key", "redis");

    expect(result).toEqual({ ok: false, error: "Only SQL can be formatted" });
  });
});
