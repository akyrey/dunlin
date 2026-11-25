import type { PaletteItem } from "@roost/palette";

import type { ConnectionProfile, SavedQuery, SessionInfo, TableRef } from "../../ipc";
import { type ShortcutId, shortcutLabel } from "../shortcuts/registry";

export type CommandGroup = "Actions" | "Connections" | "Tables" | "Saved queries";

export type PaletteCommand = Omit<PaletteItem, "group" | "hint"> & {
  group: CommandGroup;
  shortcut?: ShortcutId;
};

/** The palette's row for a command: its shortcut shown the platform's way. */
export function toPaletteItem(command: PaletteCommand, mac?: boolean): PaletteItem {
  const { shortcut, ...item } = command;
  return shortcut ? { ...item, hint: shortcutLabel(shortcut, mac) } : item;
}

export type PaletteContext = {
  profiles: readonly ConnectionProfile[];
  /** By connection id. */
  sessions: Readonly<Record<string, SessionInfo>>;
  activeConnectionId: string | null;
  tables: readonly TableRef[];
  savedQueries: readonly SavedQuery[];
};

export type PaletteHandlers = {
  newConnection: () => void;
  openSettings: () => void;
  openShortcuts: () => void;
  checkForUpdates: () => void;
  newTab: (sessionId: string) => void;
  refreshSchema: (sessionId: string) => void;
  queryStats: (sessionId: string) => void;
  disconnect: (connectionId: string) => void;
  openConnection: (profile: ConnectionProfile) => void;
  openTable: (sessionId: string, table: TableRef) => void;
  openSavedQuery: (query: SavedQuery) => void;
};

function actions(context: PaletteContext, on: PaletteHandlers): PaletteCommand[] {
  const active = context.activeConnectionId;
  const session = active ? context.sessions[active] : undefined;
  const always: PaletteCommand[] = [
    { id: "new-connection", group: "Actions", label: "New connection", run: on.newConnection },
    {
      id: "settings",
      group: "Actions",
      label: "Settings",
      shortcut: "settings",
      run: on.openSettings,
    },
    {
      id: "shortcuts",
      group: "Actions",
      label: "Keyboard shortcuts",
      shortcut: "shortcuts",
      run: on.openShortcuts,
    },
    {
      id: "check-updates",
      group: "Actions",
      label: "Check for updates",
      run: on.checkForUpdates,
    },
  ];
  if (!session || !active) return always;
  return [
    {
      id: "new-tab",
      group: "Actions",
      label: "New editor tab",
      shortcut: "new-tab",
      run: () => {
        on.newTab(session.sessionId);
      },
    },
    {
      id: "refresh-schema",
      group: "Actions",
      label: "Refresh schema",
      run: () => {
        on.refreshSchema(session.sessionId);
      },
    },
    ...(session.capabilities.queryStats
      ? [
          {
            id: "query-stats",
            group: "Actions" as const,
            label: "Query statistics",
            run: () => {
              on.queryStats(session.sessionId);
            },
          },
        ]
      : []),
    {
      id: "disconnect",
      group: "Actions",
      label: "Disconnect",
      run: () => {
        on.disconnect(active);
      },
    },
    ...always,
  ];
}

const qualified = (table: TableRef): string =>
  [table.namespace.catalog, table.namespace.schema].filter(Boolean).join(".");

/** Everything the palette offers in the current state (task 8.9). */
export function paletteCommands(context: PaletteContext, on: PaletteHandlers): PaletteCommand[] {
  const active = context.activeConnectionId;
  const session = active ? context.sessions[active] : undefined;
  const connections = context.profiles.map((profile): PaletteCommand => ({
    id: `connection:${profile.id}`,
    group: "Connections",
    label: profile.name,
    detail: context.sessions[profile.id] ? "open" : undefined,
    run: () => {
      on.openConnection(profile);
    },
  }));
  const tables = session
    ? context.tables.map((table): PaletteCommand => ({
        id: `table:${qualified(table)}.${table.name}`,
        group: "Tables",
        label: table.name,
        detail: qualified(table) || undefined,
        run: () => {
          on.openTable(session.sessionId, table);
        },
      }))
    : [];
  const queries = context.savedQueries.map((query): PaletteCommand => ({
    id: `query:${query.id}`,
    group: "Saved queries",
    label: query.name,
    detail: query.connectionId === null ? "every connection" : undefined,
    run: () => {
      on.openSavedQuery(query);
    },
  }));
  return [...actions(context, on), ...connections, ...tables, ...queries];
}
