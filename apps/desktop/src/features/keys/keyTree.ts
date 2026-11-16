/** Groups scanned keys into folders by the usual Redis namespace separator. */
import type { KeySummary } from "../../ipc";

export const KEY_SEPARATOR = ":";
const EMPTY_SEGMENT = "(empty)";

export type KeyFolder = {
  kind: "folder";
  label: string;
  /** The prefix the folder stands for, separator included, e.g. `user:`. */
  path: string;
  /** Keys under the folder, at any depth. */
  count: number;
  children: KeyNode[];
};

export type KeyLeaf = { kind: "key"; label: string; summary: KeySummary };

export type KeyNode = KeyFolder | KeyLeaf;

export type KeyRow =
  | { kind: "folder"; depth: number; label: string; path: string; count: number; expanded: boolean }
  | { kind: "key"; depth: number; label: string; summary: KeySummary };

type Entry = { rest: string[]; summary: KeySummary };

function leafLabel(segment: string, summary: KeySummary): string {
  return segment === "" ? summary.key.name : segment;
}

function byLabel(a: KeyNode, b: KeyNode): number {
  if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
  return a.label.localeCompare(b.label);
}

function buildLevel(entries: Entry[], prefix: string): KeyNode[] {
  const groups = new Map<string, Entry[]>();
  const leaves: KeyLeaf[] = [];
  for (const { rest, summary } of entries) {
    const [head = "", ...tail] = rest;
    if (tail.length === 0) {
      leaves.push({ kind: "key", label: leafLabel(head, summary), summary });
    } else {
      groups.set(head, [...(groups.get(head) ?? []), { rest: tail, summary }]);
    }
  }
  const folders = [...groups].map(([segment, inner]): KeyFolder => {
    const path = `${prefix}${segment}${KEY_SEPARATOR}`;
    return {
      kind: "folder",
      label: segment === "" ? EMPTY_SEGMENT : segment,
      path,
      count: inner.length,
      children: buildLevel(inner, path),
    };
  });
  return [...folders, ...leaves].sort(byLabel);
}

/** A tree of the keys split on `:`; a key and a folder of the same name stay apart. */
export function buildKeyTree(keys: KeySummary[]): KeyNode[] {
  return buildLevel(
    keys.map((summary) => ({ rest: summary.key.name.split(KEY_SEPARATOR), summary })),
    "",
  );
}

/** The rows on screen: every node whose ancestors are all in `expanded` (by folder path). */
export function visibleRows(nodes: KeyNode[], expanded: ReadonlySet<string>, depth = 0): KeyRow[] {
  return nodes.flatMap((node): KeyRow[] => {
    if (node.kind === "key") {
      return [{ kind: "key", depth, label: node.label, summary: node.summary }];
    }
    const open = expanded.has(node.path);
    const row: KeyRow = {
      kind: "folder",
      depth,
      label: node.label,
      path: node.path,
      count: node.count,
      expanded: open,
    };
    return open ? [row, ...visibleRows(node.children, expanded, depth + 1)] : [row];
  });
}
