import { CompletionContext } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";

import { completeCommand, redis, redisLanguage } from "./redisLanguage";

/** Each token's text and style, as the editor highlights them. */
function tokens(doc: string): [string, string][] {
  const tree = redis.parser.parse(doc);
  const found: [string, string][] = [];
  tree.iterate({
    enter: (node) => {
      if (node.type.isTop) return;
      found.push([doc.slice(node.from, node.to), node.name]);
    },
  });
  return found;
}

function complete(doc: string, explicit = false) {
  const state = EditorState.create({ doc });
  return completeCommand(new CompletionContext(state, doc.length, explicit));
}

describe("Redis command completion", () => {
  it("completes the command word at the start of a line only", () => {
    const result = complete("SET a 1\n  hge");

    expect(result?.from).toBe("SET a 1\n  ".length);
    expect(result?.options.some((o) => o.label === "HGETALL")).toBe(true);
    expect(complete("GET some")).toBeNull();
  });

  it("offers every command on an empty line only when asked", () => {
    expect(complete("")).toBeNull();
    expect(complete("", true)?.options.length).toBeGreaterThan(50);
  });
});

describe("Redis highlighting", () => {
  it("marks the command word, quoted strings, numbers and plain words", () => {
    expect(tokens('SET "a key" 12 -3.5 plain')).toEqual([
      ["SET", "keyword"],
      ['"a key"', "string"],
      ["12", "number"],
      ["-3.5", "number"],
    ]);
  });

  it("treats each line's first word as its command", () => {
    expect(tokens("GET a\nhgetall b")).toEqual([
      ["GET", "keyword"],
      ["hgetall", "keyword"],
    ]);
  });

  it("reads escaped quotes inside strings and an unclosed string to the end", () => {
    expect(tokens(String.raw`SET k 'it\'s' "open`)).toEqual([
      ["SET", "keyword"],
      [String.raw`'it\'s'`, "string"],
      ['"open', "string"],
    ]);
  });

  it("marks # and // lines as comments, but not a # inside a command", () => {
    expect(tokens("# note\n// also\nGET #tag")).toEqual([
      ["# note", "comment"],
      ["// also", "comment"],
      ["GET", "keyword"],
    ]);
  });

  it("is an editor extension with completion", () => {
    expect(redisLanguage()).toHaveLength(2);
  });
});
