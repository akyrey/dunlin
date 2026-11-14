import type { Value } from "../ipc";
import { decodeBase64, hexDump, imageType } from "./binary";
import { exactNumber, prettyJson, rowsToJson, toJsonValue } from "./json";
import { prettyXml } from "./xml";

describe("JSON view", () => {
  it.each([
    ["42", 42],
    ["-9007199254740991", -9007199254740991],
    ["9007199254740993", null],
    ["12.50", 12.5],
    ["0.1", 0.1],
    ["12345678901234567890.123456789", null],
    ["1e400", null],
    ["+3", 3],
  ])("exactNumber(%s) is %s", (text, expected) => {
    expect(exactNumber(text)).toBe(expected);
  });

  it("keeps exact numbers, strings the rest, embeds valid JSON and base64 for binary", () => {
    const row: Value[] = [
      { t: "int", v: "9007199254740993" },
      { t: "decimal", v: "1.5" },
      { t: "json", v: '{"a": [1]}' },
      { t: "json", v: "not json" },
      { t: "json", v: '{"cut', truncated: true },
      { t: "bytes", v: "3q0=", len: 2 },
      { t: "bool", v: true },
      { t: "null" },
      { t: "array", v: [{ t: "int", v: "1" }] },
      { t: "unknown", type: "hstore", hex: "ab" },
      { t: "date", v: "2026-11-14" },
    ];
    const names = ["big", "dec", "json", "bad", "cut", "bin", "flag", "none", "arr", "odd", "day"];

    expect(JSON.parse(rowsToJson(names, [row]))).toEqual([
      {
        big: "9007199254740993",
        dec: 1.5,
        json: { a: [1] },
        bad: "not json",
        cut: '{"cut',
        bin: "3q0=",
        flag: true,
        none: null,
        arr: [1],
        odd: "ab",
        day: "2026-11-14",
      },
    ]);
    expect(toJsonValue({ t: "unknown", type: "x" })).toBeNull();
  });

  it("pretty-prints JSON or says it cannot", () => {
    expect(prettyJson('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(prettyJson("{oops")).toBeNull();
  });
});

describe("cell viewer helpers", () => {
  it("dumps bytes as hex with offsets and printable characters", () => {
    const bytes = decodeBase64(btoa("Hello, binary world!"));

    expect(hexDump(bytes).split("\n")).toEqual([
      "00000000  48 65 6c 6c 6f 2c 20 62 69 6e 61 72 79 20 77 6f  |Hello, binary wo|",
      "00000010  72 6c 64 21                                      |rld!|",
    ]);
  });

  it("recognises raster images but never SVG", () => {
    const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0]);
    const webp = new TextEncoder().encode("RIFF\0\0\0\0WEBPVP8 ");
    const riffWav = new TextEncoder().encode("RIFF\0\0\0\0WAVEfmt ");

    expect(imageType(png)).toBe("image/png");
    expect(imageType(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]))).toBe("image/jpeg");
    expect(imageType(webp)).toBe("image/webp");
    expect(imageType(riffWav)).toBeNull();
    expect(imageType(new TextEncoder().encode("<svg onload='x'>"))).toBeNull();
  });

  it("pretty-prints XML as text and refuses what does not parse", () => {
    expect(prettyXml('<a x="1"><b>t &amp; u</b><c/><!-- n --><d><e/></d></a>')).toBe(
      '<a x="1">\n  <b>t &amp; u</b>\n  <c/>\n  <!-- n -->\n  <d>\n    <e/>\n  </d>\n</a>',
    );
    expect(prettyXml("<a><b></a>")).toBeNull();
  });
});
