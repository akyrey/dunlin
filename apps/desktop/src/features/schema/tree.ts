/** Flattens the schema tree into the rows a virtualised list renders. Pure, so it is cheap to
 * test and to recompute for thousands of objects. */
import type { AppError, Namespace, NamespaceRef, ObjectSummary } from "../../ipc";
import {
  type Load,
  SESSION_KEY,
  type SessionTree,
  groupNodeKey,
  isOpenByDefault,
  namespaceKey,
  namespaceNodeKey,
} from "./schemaStore";

export type TreeSession = { sessionId: string; label: string; tree: SessionTree };

type Base = { key: string; depth: number; sessionId: string };

export type TreeRow =
  | (Base & { kind: "session"; label: string; expanded: boolean })
  | (Base & { kind: "namespace"; label: string; expanded: boolean; namespace: NamespaceRef })
  | (Base & {
      kind: "group";
      label: string;
      expanded: boolean;
      count: number;
      namespace: NamespaceRef;
    })
  | (Base & { kind: "object"; object: ObjectSummary; namespace: NamespaceRef })
  | (Base & { kind: "loading" })
  | (Base & { kind: "error"; error: AppError })
  | (Base & { kind: "empty"; label: string });

export type TreeOptions = { filter: string; showSystem: boolean };

const GROUPS = [
  { group: "tables", label: "Tables", matches: (o: ObjectSummary) => o.kind === "table" },
  { group: "views", label: "Views", matches: (o: ObjectSummary) => o.kind !== "table" },
] as const;

function expanded(tree: SessionTree, key: string): boolean {
  return tree.expanded[key] ?? isOpenByDefault(key);
}

export function visibleRows(sessions: TreeSession[], options: TreeOptions): TreeRow[] {
  return sessions.flatMap((session) => sessionRows(session, options));
}

function sessionRows({ sessionId, label, tree }: TreeSession, options: TreeOptions): TreeRow[] {
  const open = expanded(tree, SESSION_KEY);
  const rows: TreeRow[] = [
    {
      kind: "session",
      key: `${sessionId}/${SESSION_KEY}`,
      depth: 0,
      sessionId,
      label,
      expanded: open,
    },
  ];
  if (!open) return rows;
  const load = tree.namespaces;
  const status = statusRow(sessionId, `${sessionId}/namespaces`, 1, load);
  if (status) return [...rows, status];
  if (load?.status !== "loaded") return rows;
  const namespaces = load.data.filter((n) => options.showSystem || !n.system);
  if (namespaces.length === 0) {
    return [...rows, empty(sessionId, `${sessionId}/namespaces`, 1, "No databases")];
  }
  return [...rows, ...namespaces.flatMap((n) => namespaceRows(sessionId, tree, n, options))];
}

function namespaceRows(
  sessionId: string,
  tree: SessionTree,
  { namespace, name }: Namespace,
  options: TreeOptions,
): TreeRow[] {
  const nodeKey = namespaceNodeKey(namespace);
  const open = expanded(tree, nodeKey);
  const key = `${sessionId}/${nodeKey}`;
  const rows: TreeRow[] = [
    { kind: "namespace", key, depth: 1, sessionId, label: name, expanded: open, namespace },
  ];
  if (!open) return rows;
  const load = tree.objects[namespaceKey(namespace)];
  const status = statusRow(sessionId, `${key}/objects`, 2, load);
  if (status) return [...rows, status];
  if (load?.status !== "loaded") return rows;
  const filter = options.filter.trim().toLowerCase();
  const objects = filter
    ? load.data.filter((o) => o.name.toLowerCase().includes(filter))
    : load.data;
  if (objects.length === 0) {
    return [...rows, empty(sessionId, `${key}/objects`, 2, filter ? "No matches" : "No tables")];
  }
  for (const { group, label, matches } of GROUPS) {
    const members = objects.filter(matches);
    if (members.length === 0) continue;
    const groupKey = groupNodeKey(namespace, group);
    const groupOpen = expanded(tree, groupKey);
    rows.push({
      kind: "group",
      key: `${sessionId}/${groupKey}`,
      depth: 2,
      sessionId,
      label,
      expanded: groupOpen,
      count: members.length,
      namespace,
    });
    if (!groupOpen) continue;
    for (const object of members) {
      rows.push({
        kind: "object",
        key: `${sessionId}/${groupKey}/${object.name}`,
        depth: 3,
        sessionId,
        object,
        namespace,
      });
    }
  }
  return rows;
}

function statusRow<T>(
  sessionId: string,
  key: string,
  depth: number,
  load: Load<T> | undefined,
): TreeRow | null {
  if (load?.status === "loading") return { kind: "loading", key, depth, sessionId };
  if (load?.status === "failed") return { kind: "error", key, depth, sessionId, error: load.error };
  return null;
}

function empty(sessionId: string, key: string, depth: number, label: string): TreeRow {
  return { kind: "empty", key, depth, sessionId, label };
}
