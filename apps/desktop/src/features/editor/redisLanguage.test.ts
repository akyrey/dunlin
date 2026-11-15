import { CompletionContext } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";

import { completeCommand } from "./redisLanguage";

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
