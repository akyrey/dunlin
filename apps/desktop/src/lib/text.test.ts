import { lineAndColumn, statementAt } from "./text";

describe("lineAndColumn", () => {
  it("counts from one", () => {
    expect(lineAndColumn("SELECT 1", 0)).toEqual({ line: 1, column: 1 });
    expect(lineAndColumn("SELECT 1;\nSELECT x", 17)).toEqual({ line: 2, column: 8 });
  });

  it("clamps offsets outside the text", () => {
    expect(lineAndColumn("ab", 99)).toEqual({ line: 1, column: 3 });
    expect(lineAndColumn("ab", -4)).toEqual({ line: 1, column: 1 });
  });
});

describe("statementAt", () => {
  const ranges = [
    { start: 0, end: 8 },
    { start: 10, end: 18 },
  ];

  it("picks the statement containing the cursor", () => {
    expect(statementAt(ranges, 12)).toEqual({ start: 10, end: 18 });
    expect(statementAt(ranges, 8)).toEqual({ start: 0, end: 8 });
  });

  it("picks the previous statement when the cursor is after its semicolon", () => {
    expect(statementAt(ranges, 9)).toEqual({ start: 0, end: 8 });
    expect(statementAt(ranges, 30)).toEqual({ start: 10, end: 18 });
  });

  it("picks the next statement when the cursor is before everything", () => {
    expect(statementAt([{ start: 4, end: 9 }], 1)).toEqual({ start: 4, end: 9 });
  });

  it("returns null without statements", () => {
    expect(statementAt([], 0)).toBeNull();
  });
});
