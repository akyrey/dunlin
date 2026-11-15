import { formatTtl } from "./ttl";

describe("formatTtl", () => {
  it("reads naturally at every scale", () => {
    expect(formatTtl(null)).toBe("no expiry");
    expect(formatTtl(12_400)).toBe("12 s");
    expect(formatTtl(30 * 60_000)).toBe("30 min");
    expect(formatTtl(5 * 3_600_000)).toBe("5 h");
    expect(formatTtl(9 * 86_400_000)).toBe("9 d");
  });
});
