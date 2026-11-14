import type { Value } from "../ipc";
import { formatValue, isNumericCategory } from "./value";

describe("formatValue", () => {
  it.each<[Value, string]>([
    [{ t: "bool", v: true }, "true"],
    [{ t: "int", v: "9007199254740993" }, "9007199254740993"],
    [{ t: "uint", v: "18446744073709551615" }, "18446744073709551615"],
    [{ t: "float", v: "NaN" }, "NaN"],
    [{ t: "decimal", v: "12345678901234567890.123456789" }, "12345678901234567890.123456789"],
    [{ t: "text", v: "hello" }, "hello"],
    [{ t: "json", v: '{"a":1}' }, '{"a":1}'],
    [{ t: "xml", v: "<a/>" }, "<a/>"],
    [{ t: "uuid", v: "6f1c" }, "6f1c"],
    [{ t: "date", v: "2026-11-14" }, "2026-11-14"],
    [{ t: "time", v: "10:00" }, "10:00"],
    [{ t: "timestamp", v: "2026-11-14T10:00:00" }, "2026-11-14T10:00:00"],
    [{ t: "timestamptz", v: "2026-11-14T10:00:00Z" }, "2026-11-14T10:00:00Z"],
    [{ t: "interval", v: "1 day" }, "1 day"],
  ])("shows %j as %s", (value, expected) => {
    expect(formatValue(value).text).toBe(expected);
  });

  it("marks NULL as null, distinct from the empty string", () => {
    expect(formatValue({ t: "null" })).toEqual({ text: "NULL", isNull: true, truncated: false });
    expect(formatValue({ t: "text", v: "" })).toEqual({
      text: "",
      isNull: false,
      truncated: false,
    });
  });

  it("shows bytes as hex with their size", () => {
    const result = formatValue({ t: "bytes", v: "3q2+7w==", len: 4 });

    expect(result.text).toBe("0xdeadbeef (4 B)");
  });

  it("shortens long byte previews and reports the true size", () => {
    const result = formatValue({
      t: "bytes",
      v: "AAAAAAAAAAAAAAAAAAAAAAAAAAAA",
      len: 70000,
      truncated: true,
    });

    expect(result.text).toBe("0x00000000000000000000000000000000… (68.4 KiB)");
    expect(result.truncated).toBe(true);
  });

  it("flags truncated text", () => {
    expect(formatValue({ t: "text", v: "abc", truncated: true })).toEqual({
      text: "abc…",
      isNull: false,
      truncated: true,
    });
  });

  it("renders arrays recursively", () => {
    const value: Value = {
      t: "array",
      v: [{ t: "int", v: "1" }, { t: "null" }, { t: "text", v: "x" }],
    };

    expect(formatValue(value).text).toBe("[1, NULL, x]");
  });

  it("shows unknown values by text, then hex, then type", () => {
    expect(formatValue({ t: "unknown", type: "inet", text: "10.0.0.1" }).text).toBe("10.0.0.1");
    expect(formatValue({ t: "unknown", type: "geometry", hex: "0101" }).text).toBe("0x0101");
    expect(formatValue({ t: "unknown", type: "geometry" }).text).toBe("<geometry>");
  });
});

describe("isNumericCategory", () => {
  it("right-aligns numbers only", () => {
    expect(isNumericCategory("integer")).toBe(true);
    expect(isNumericCategory("float")).toBe(true);
    expect(isNumericCategory("decimal")).toBe(true);
    expect(isNumericCategory("text")).toBe(false);
  });
});

describe("formatSize", () => {
  it("uses binary units", async () => {
    const { formatSize } = await import("./value");

    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(2048)).toBe("2.0 KiB");
    expect(formatSize(3 * 1024 * 1024)).toBe("3.0 MiB");
  });
});
