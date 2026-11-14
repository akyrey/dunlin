import { sql } from "@codemirror/lang-sql";

import { dialectFor } from "./dialects";

/** Node names of every token `text` parses into, with the text of each. */
function tokens(driver: "postgres" | "mysql" | "sqlite", text: string): [string, string][] {
  const tree = sql({ dialect: dialectFor(driver) }).language.parser.parse(text);
  const found: [string, string][] = [];
  tree.iterate({
    enter: (node) => {
      if (node.node.firstChild === null && node.name !== "Statement") {
        found.push([node.name, text.slice(node.from, node.to)]);
      }
    },
  });
  return found;
}

describe("dialectFor", () => {
  it("reads PostgreSQL dollar-quoted strings as one string", () => {
    expect(tokens("postgres", "SELECT $tag$it's -- not a comment$tag$")).toContainEqual([
      "String",
      "$tag$it's -- not a comment$tag$",
    ]);
  });

  it("reads MySQL backticks as identifiers and # as a comment", () => {
    const found = tokens("mysql", "SELECT `my col` FROM t # note");

    expect(found).toContainEqual(["QuotedIdentifier", "`my col`"]);
    expect(found).toContainEqual(["LineComment", "# note"]);
  });

  it("highlights keywords, strings and comments in SQLite", () => {
    const found = tokens("sqlite", "SELECT 'a' /* c */ FROM \"t\"");

    expect(found).toEqual(
      expect.arrayContaining([
        ["Keyword", "SELECT"],
        ["String", "'a'"],
        ["BlockComment", "/* c */"],
        ["QuotedIdentifier", '"t"'],
      ]),
    );
  });

  it("does not treat a PostgreSQL double-quoted name as a string", () => {
    expect(tokens("postgres", 'SELECT "Name"')).toContainEqual(["QuotedIdentifier", '"Name"']);
  });
});
