/** Pretty-printed XML for the cell viewer, or `null` when it does not parse. The document is
 * only parsed and re-serialised as text; nothing in it is rendered or run. */
const INDENT = "  ";

export function prettyXml(text: string): string | null {
  const parsed = new DOMParser().parseFromString(text, "application/xml");
  if (parsed.getElementsByTagName("parsererror").length > 0) return null;
  const lines: string[] = [];
  const walk = (node: Node, depth: number) => {
    const pad = INDENT.repeat(depth);
    if (node.nodeType === Node.TEXT_NODE) {
      const content = node.textContent?.trim();
      if (content) lines.push(pad + escapeText(content));
      return;
    }
    if (node.nodeType === Node.COMMENT_NODE) {
      lines.push(`${pad}<!--${node.textContent ?? ""}-->`);
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const element = node as Element;
    const attributes = Array.from(
      element.attributes,
      (a) => ` ${a.name}="${escapeAttribute(a.value)}"`,
    ).join("");
    const children = Array.from(element.childNodes).filter(
      (child) => child.nodeType !== Node.TEXT_NODE || child.textContent?.trim(),
    );
    const only = children[0];
    if (children.length === 0) {
      lines.push(`${pad}<${element.tagName}${attributes}/>`);
    } else if (children.length === 1 && only?.nodeType === Node.TEXT_NODE) {
      lines.push(
        `${pad}<${element.tagName}${attributes}>${escapeText(only.textContent?.trim() ?? "")}</${element.tagName}>`,
      );
    } else {
      lines.push(`${pad}<${element.tagName}${attributes}>`);
      for (const child of children) walk(child, depth + 1);
      lines.push(`${pad}</${element.tagName}>`);
    }
  };
  walk(parsed.documentElement, 0);
  return lines.join("\n");
}

function escapeText(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

function escapeAttribute(text: string): string {
  return escapeText(text).replaceAll('"', "&quot;");
}
