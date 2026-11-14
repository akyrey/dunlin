import { create } from "zustand";

import {
  type AppError,
  type Namespace,
  type NamespaceRef,
  type ObjectSummary,
  schemaNamespaces,
  schemaObjects,
  schemaRefresh,
  toAppError,
} from "../../ipc";

export type Load<T> =
  { status: "loading" } | { status: "loaded"; data: T } | { status: "failed"; error: AppError };

/** One open session's tree: what is loaded and which nodes are expanded. */
export type SessionTree = {
  namespaces?: Load<Namespace[]>;
  /** By `namespaceKey`. */
  objects: Record<string, Load<ObjectSummary[]>>;
  /** Node keys the user expanded or collapsed; others use their default. */
  expanded: Record<string, boolean>;
};

type SchemaState = {
  /** By session id. */
  trees: Record<string, SessionTree>;
  filter: string;
  showSystem: boolean;
  setFilter: (filter: string) => void;
  setShowSystem: (show: boolean) => void;
  /** Expands or collapses a node, loading its children on first expand. */
  toggle: (sessionId: string, key: string, namespace?: NamespaceRef) => Promise<void>;
  /** Reloads what is loaded; the backend cache was already cleared (e.g. after DDL). */
  reload: (sessionId: string) => Promise<void>;
  /** Clears the backend cache, then reloads. */
  refresh: (sessionId: string) => Promise<void>;
  dropSession: (sessionId: string) => void;
};

export const SESSION_KEY = "session";

export function namespaceKey(namespace: NamespaceRef): string {
  return JSON.stringify([namespace.catalog, namespace.schema]);
}

export const namespaceNodeKey = (namespace: NamespaceRef): string =>
  `ns:${namespaceKey(namespace)}`;

export const groupNodeKey = (namespace: NamespaceRef, group: "tables" | "views"): string =>
  `${group}:${namespaceKey(namespace)}`;

const emptyTree = (): SessionTree => ({ objects: {}, expanded: {} });

const failed = (caught: unknown): { status: "failed"; error: AppError } => ({
  status: "failed",
  error: toAppError(caught),
});

export const useSchema = create<SchemaState>()((set, get) => {
  const tree = (sessionId: string) => get().trees[sessionId] ?? emptyTree();

  /** Replaces part of a session's tree, unless the session was dropped meanwhile. */
  const patch = (
    sessionId: string,
    change: (current: SessionTree) => SessionTree,
    create = false,
  ) => {
    const current = get().trees[sessionId];
    if (!current && !create) return;
    set({ trees: { ...get().trees, [sessionId]: change(current ?? emptyTree()) } });
  };

  const loadObjects = async (sessionId: string, namespace: NamespaceRef) => {
    const key = namespaceKey(namespace);
    const setLoad = (load: Load<ObjectSummary[]>) => {
      patch(sessionId, (t) => ({ ...t, objects: { ...t.objects, [key]: load } }));
    };
    setLoad({ status: "loading" });
    try {
      setLoad({ status: "loaded", data: await schemaObjects(sessionId, namespace) });
    } catch (caught) {
      setLoad(failed(caught));
    }
  };

  const loadNamespaces = async (sessionId: string) => {
    const setLoad = (load: Load<Namespace[]>) => {
      patch(sessionId, (t) => ({ ...t, namespaces: load }));
    };
    setLoad({ status: "loading" });
    let namespaces: Namespace[];
    try {
      namespaces = await schemaNamespaces(sessionId);
    } catch (caught) {
      setLoad(failed(caught));
      return;
    }
    if (!get().trees[sessionId]) return;
    setLoad({ status: "loaded", data: namespaces });
    // A database with a single namespace (SQLite's main) opens straight to its objects.
    const visible = namespaces.filter((n) => get().showSystem || !n.system);
    const only = visible.length === 1 ? visible[0] : undefined;
    if (only && tree(sessionId).expanded[namespaceNodeKey(only.namespace)] === undefined) {
      patch(sessionId, (t) => ({
        ...t,
        expanded: { ...t.expanded, [namespaceNodeKey(only.namespace)]: true },
      }));
      await loadObjects(sessionId, only.namespace);
    }
  };

  return {
    trees: {},
    filter: "",
    showSystem: false,

    setFilter: (filter) => {
      set({ filter });
    },

    setShowSystem: (showSystem) => {
      set({ showSystem });
    },

    toggle: async (sessionId, key, namespace) => {
      const current = tree(sessionId);
      const expanding = !(current.expanded[key] ?? isOpenByDefault(key));
      patch(sessionId, (t) => ({ ...t, expanded: { ...t.expanded, [key]: expanding } }), true);
      if (!expanding) return;
      if (key === SESSION_KEY && !current.namespaces) await loadNamespaces(sessionId);
      if (namespace && !current.objects[namespaceKey(namespace)]) {
        await loadObjects(sessionId, namespace);
      }
    },

    reload: async (sessionId) => {
      const current = get().trees[sessionId];
      if (!current) return;
      const loads: Promise<void>[] = [];
      if (current.namespaces) loads.push(loadNamespaces(sessionId));
      const loaded = current.namespaces?.status === "loaded" ? current.namespaces.data : [];
      for (const namespace of loaded) {
        if (current.objects[namespaceKey(namespace.namespace)]) {
          loads.push(loadObjects(sessionId, namespace.namespace));
        }
      }
      await Promise.all(loads);
    },

    refresh: async (sessionId) => {
      try {
        await schemaRefresh(sessionId);
      } catch (caught) {
        patch(sessionId, (t) => ({ ...t, namespaces: failed(caught) }));
        return;
      }
      await get().reload(sessionId);
    },

    dropSession: (sessionId) => {
      set({
        trees: Object.fromEntries(Object.entries(get().trees).filter(([k]) => k !== sessionId)),
      });
    },
  };
});

/** Table and view groups start open; sessions and namespaces start closed. */
export function isOpenByDefault(key: string): boolean {
  return key.startsWith("tables:") || key.startsWith("views:");
}
