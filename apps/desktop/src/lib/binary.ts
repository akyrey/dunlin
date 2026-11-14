/** Binary values in the cell viewer (task 6.10): hex dump and image detection. */

export function decodeBase64(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

const BYTES_PER_LINE = 16;
/** Lines of hex dump shown before "…". */
export const MAX_DUMP_LINES = 4096;

/** `00000000  de ad be ef ...  |....|`, at most `MAX_DUMP_LINES` lines. */
export function hexDump(bytes: Uint8Array): string {
  const lines: string[] = [];
  for (
    let offset = 0;
    offset < bytes.length && lines.length < MAX_DUMP_LINES;
    offset += BYTES_PER_LINE
  ) {
    const chunk = bytes.subarray(offset, offset + BYTES_PER_LINE);
    const hex = Array.from(chunk, (b) => b.toString(16).padStart(2, "0")).join(" ");
    const ascii = Array.from(chunk, (b) =>
      b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".",
    ).join("");
    lines.push(
      `${offset.toString(16).padStart(8, "0")}  ${hex.padEnd(BYTES_PER_LINE * 3 - 1)}  |${ascii}|`,
    );
  }
  if (bytes.length > MAX_DUMP_LINES * BYTES_PER_LINE) lines.push("…");
  return lines.join("\n");
}

const SIGNATURES: { type: string; bytes: number[] }[] = [
  { type: "image/png", bytes: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] },
  { type: "image/jpeg", bytes: [0xff, 0xd8, 0xff] },
  { type: "image/gif", bytes: [0x47, 0x49, 0x46, 0x38] },
  { type: "image/webp", bytes: [0x52, 0x49, 0x46, 0x46] },
];

/** The image type of common raster formats by their signature. SVG is never previewed: it is
 * a document that can hold script. */
export function imageType(bytes: Uint8Array): string | null {
  const match = SIGNATURES.find(({ bytes: signature }) =>
    signature.every((b, i) => bytes[i] === b),
  );
  if (match?.type === "image/webp") {
    const tag = String.fromCharCode(...bytes.subarray(8, 12));
    return tag === "WEBP" ? match.type : null;
  }
  return match?.type ?? null;
}
