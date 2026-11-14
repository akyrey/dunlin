import { useEffect, useMemo, useState } from "react";

import { type NamespaceRef, type SessionInfo, type TableRef, schemaObjects } from "../../ipc";
import { type SessionTree, namespaceKey, useSchema } from "../schema/schemaStore";

const BROWSABLE = new Set(["table", "view", "materialized_view"]);

function fromTree(tree: SessionTree | undefined): TableRef[] {
  if (!tree) return [];
  return Object.entries(tree.objects).flatMap(([key, load]) => {
    if (load.status !== "loaded") return [];
    const [catalog, schema] = JSON.parse(key) as [string | null, string | null];
    const namespace: NamespaceRef = { catalog, schema };
    return load.data.filter((o) => BROWSABLE.has(o.kind)).map((o) => ({ namespace, name: o.name }));
  });
}

/** Tables and views of the session that the palette can open: those loaded in the tree, and the
 * default namespace's even when the tree has not opened it. */
export function useSessionTables(session: SessionInfo | null): TableRef[] {
  const tree = useSchema((s) => (session ? s.trees[session.sessionId] : undefined));
  const [fetched, setFetched] = useState<TableRef[]>([]);
  const namespace = session?.defaultNamespace ?? null;
  const loadedDefault = namespace !== null && tree?.objects[namespaceKey(namespace)] !== undefined;

  useEffect(() => {
    if (!session || !namespace || loadedDefault) return;
    let current = true;
    // A failure leaves the palette with the tree's tables; the tree shows the error itself.
    schemaObjects(session.sessionId, namespace).then(
      (objects) => {
        if (current)
          setFetched(
            objects.filter((o) => BROWSABLE.has(o.kind)).map((o) => ({ namespace, name: o.name })),
          );
      },
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, [session, namespace, loadedDefault]);

  return useMemo(() => {
    const seen = new Set<string>();
    return [...fromTree(tree), ...(loadedDefault ? [] : fetched)].filter((table) => {
      const key = JSON.stringify([table.namespace.catalog, table.namespace.schema, table.name]);
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }, [tree, fetched, loadedDefault]);
}
