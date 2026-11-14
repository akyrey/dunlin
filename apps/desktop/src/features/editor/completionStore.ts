/** The schema autocomplete offers per session (task 5.4): table and column names of every
 * non-system namespace, nested by schema as `@codemirror/lang-sql` expects. */
import type { SQLNamespace } from "@codemirror/lang-sql";
import { create } from "zustand";

import {
  type AppError,
  type CompletionSchema,
  type SessionInfo,
  schemaCompletion,
  schemaNamespaces,
  toAppError,
} from "../../ipc";

export type SessionCompletion = {
  schema: SQLNamespace;
  /** Unqualified names complete against this schema. */
  defaultSchema: string | undefined;
  /** Why the schema could not be loaded; keywords still complete. */
  error: AppError | null;
};

type CompletionState = {
  /** By session id. */
  completions: Record<string, SessionCompletion>;
  /** Sessions whose default schema is known, for reloads. */
  defaults: Record<string, string | undefined>;
  load: (session: SessionInfo) => Promise<void>;
  /** Loads again after DDL or a refresh; does nothing for a session never loaded. */
  reload: (sessionId: string) => Promise<void>;
  dropSession: (sessionId: string) => void;
};

export function toNamespace(schemas: CompletionSchema[]): SQLNamespace {
  const nested: Record<string, Record<string, string[]>> = {};
  for (const { tables } of schemas) {
    for (const table of tables) {
      const schema = table.schema ?? "";
      nested[schema] = { ...nested[schema], [table.name]: table.columns };
    }
  }
  return nested;
}

async function fetchCompletion(sessionId: string): Promise<SQLNamespace> {
  const namespaces = (await schemaNamespaces(sessionId)).filter((n) => !n.system);
  const schemas = await Promise.all(
    namespaces.map((n) => schemaCompletion(sessionId, n.namespace)),
  );
  return toNamespace(schemas);
}

export const useCompletion = create<CompletionState>()((set, get) => {
  const fill = async (sessionId: string, defaultSchema: string | undefined) => {
    let completion: SessionCompletion;
    try {
      completion = { schema: await fetchCompletion(sessionId), defaultSchema, error: null };
    } catch (caught) {
      completion = { schema: {}, defaultSchema, error: toAppError(caught) };
    }
    if (!(sessionId in get().defaults)) return;
    set({ completions: { ...get().completions, [sessionId]: completion } });
  };

  return {
    completions: {},
    defaults: {},

    load: async (session) => {
      const defaultSchema = session.defaultNamespace?.schema ?? undefined;
      set({ defaults: { ...get().defaults, [session.sessionId]: defaultSchema } });
      await fill(session.sessionId, defaultSchema);
    },

    reload: async (sessionId) => {
      if (!(sessionId in get().defaults)) return;
      await fill(sessionId, get().defaults[sessionId]);
    },

    dropSession: (sessionId) => {
      const without = <T>(record: Record<string, T>) =>
        Object.fromEntries(Object.entries(record).filter(([k]) => k !== sessionId));
      set({ completions: without(get().completions), defaults: without(get().defaults) });
    },
  };
});
