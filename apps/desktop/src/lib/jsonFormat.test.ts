import { formatJson, minifyJson } from "./jsonFormat";

describe("formatJson", () => {
  it("indents objects and arrays two spaces deep", () => {
    expect(formatJson('{"a":[1,2],"b":{"c":true}}')).toBe(
      '{\n  "a": [\n    1,\n    2\n  ],\n  "b": {\n    "c": true\n  }\n}',
    );
  });

  it("keeps empty containers on one line", () => {
    expect(formatJson('{"a":{},"b":[ ]}')).toBe('{\n  "a": {},\n  "b": []\n}');
  });

  it("keeps every digit of numbers a JavaScript number cannot hold", () => {
    expect(formatJson('{"big":12345678901234567890,"d":1.50}')).toBe(
      '{\n  "big": 12345678901234567890,\n  "d": 1.50\n}',
    );
  });

  it("leaves strings untouched, including escaped quotes and structural characters", () => {
    expect(formatJson('{"k":"a \\" , : { ["}')).toBe('{\n  "k": "a \\" , : { ["\n}');
  });

  it("formats a scalar as itself", () => {
    expect(formatJson('  "x" ')).toBe('"x"');
  });

  it("returns null for text that is not JSON", () => {
    expect(formatJson("{oops")).toBeNull();
  });
});

describe("minifyJson", () => {
  it("removes whitespace outside strings and keeps it inside", () => {
    expect(minifyJson('{\n  "a b": [ 1,\n 2 ],\t"c": " x "\n}')).toBe('{"a b":[1,2],"c":" x "}');
  });

  it("keeps every digit of numbers a JavaScript number cannot hold", () => {
    expect(minifyJson("[ 12345678901234567890 , 1.50 ]")).toBe("[12345678901234567890,1.50]");
  });

  it("undoes formatJson", () => {
    const raw = '{"a":[1,{"b":"c \\"d\\""}],"e":{}}';
    expect(minifyJson(formatJson(raw) ?? "")).toBe(raw);
  });

  it("returns null for text that is not JSON", () => {
    expect(minifyJson("[1,")).toBeNull();
  });
});
