import { CompletionContext } from "@codemirror/autocomplete";
import { sql } from "@codemirror/lang-sql";
import { EditorState } from "@codemirror/state";

import { columnSource, recentSource, singleLine } from "./filterCompletion";

const stateOf = (doc: string) => EditorState.create({ doc, extensions: [sql(), singleLine] });
const contextAt = (doc: string, explicit = false) =>
  new CompletionContext(stateOf(doc), doc.length, explicit);
const labels = (result: { options: readonly { label: string }[] } | null) =>
  result?.options.map((o) => o.label) ?? null;

describe("singleLine", () => {
  it("turns pasted newlines into single spaces", () => {
    const state = stateOf("a = 1");
    const next = state.update({ changes: { from: 5, insert: "\n  AND b = 2\r\n" } }).state;
    expect(next.doc.toString()).toBe("a = 1 AND b = 2 ");
    expect(next.doc.lines).toBe(1);
  });
});

describe("columnSource", () => {
  const columns = columnSource(() => ["id", "kind", "created_at"]);

  it("offers the table's columns for the word being typed", () => {
    const result = columns(contextAt("total > 1 AND ki"));
    expect(result?.from).toBe(14);
    expect(labels(result)).toEqual(["id", "kind", "created_at"]);
  });

  it("stays quiet on an empty word unless asked, after a dot, and inside strings", () => {
    expect(columns(contextAt("a = "))).toBeNull();
    expect(labels(columns(contextAt("a = ", true)))).toHaveLength(3);
    expect(columns(contextAt("t.ki"))).toBeNull();
    expect(columns(contextAt("kind = 'lo"))).toBeNull();
  });
});

describe("recentSource", () => {
  const recent = recentSource(() => ["kind = 'login'", "Kind = 'logout'", "id > 3"]);

  it("offers every recent filter on an empty filter, replacing the whole text", () => {
    const result = recent(contextAt(""));
    expect(result?.from).toBe(0);
    expect(labels(result)).toHaveLength(3);
  });

  it("matches what is typed, ignoring case, and skips an exact match", () => {
    expect(labels(recent(contextAt("kind")))).toEqual(["kind = 'login'", "Kind = 'logout'"]);
    expect(labels(recent(contextAt("id > 3")))).toBeNull();
  });

  it("offers nothing with the cursor before the end", () => {
    const state = stateOf("kind");
    expect(recent(new CompletionContext(state, 2, false))).toBeNull();
  });
});
