/** Re-spacing JSON text without parsing its values, so numbers keep every digit and strings,
 * key order and duplicate keys stay exactly as written. `JSON.parse` only validates. */

const INDENT = "  ";
const WHITESPACE = /\s/;

function isJson(text: string): boolean {
  try {
    JSON.parse(text);
    return true;
  } catch {
    return false;
  }
}

/** The index just past the string that opens at `start`. */
function stringEnd(text: string, start: number): number {
  let i = start + 1;
  while (i < text.length && text[i] !== '"') {
    i += text[i] === "\\" ? 2 : 1;
  }
  return i + 1;
}

/** The tokens of valid JSON text: strings, punctuation, and runs of literal characters. */
function tokens(text: string): string[] {
  const out: string[] = [];
  let i = 0;
  while (i < text.length) {
    const ch = text.charAt(i);
    if (WHITESPACE.test(ch)) {
      i += 1;
    } else if (ch === '"') {
      const end = stringEnd(text, i);
      out.push(text.slice(i, end));
      i = end;
    } else if ("{}[],:".includes(ch)) {
      out.push(ch);
      i += 1;
    } else {
      let end = i;
      while (end < text.length && !/[\s{}[\],:"]/.test(text.charAt(end))) end += 1;
      out.push(text.slice(i, end));
      i = end;
    }
  }
  return out;
}

/** JSON without whitespace outside strings, or `null` when the text does not parse. */
export function minifyJson(text: string): string | null {
  return isJson(text) ? tokens(text).join("") : null;
}

/** JSON indented two spaces per level, or `null` when the text does not parse. */
export function formatJson(text: string): string | null {
  if (!isJson(text)) return null;
  const parts = tokens(text);
  let out = "";
  let depth = 0;
  const newline = () => "\n" + INDENT.repeat(depth);
  parts.forEach((token, i) => {
    const next = parts[i + 1];
    if (token === "{" || token === "[") {
      const empty = next === "}" || next === "]";
      depth += 1;
      out += empty ? token : token + newline();
    } else if (token === "}" || token === "]") {
      const prev = parts[i - 1];
      depth -= 1;
      out += prev === "{" || prev === "[" ? token : newline() + token;
    } else if (token === ",") {
      out += "," + newline();
    } else if (token === ":") {
      out += ": ";
    } else {
      out += token;
    }
  });
  return out;
}
