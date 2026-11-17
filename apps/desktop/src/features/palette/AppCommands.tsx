import { useMemo } from "react";

import type { ConnectionProfile } from "../../ipc";
import { useEditor } from "../editor/editorStore";
import { openSavedQuery } from "../saved/openSaved";
import { queriesFor, useSavedQueries } from "../saved/savedQueriesStore";
import { useSchema } from "../schema/schemaStore";
import { disconnect } from "../sessions/disconnect";
import { useSessions } from "../sessions/sessionsStore";
import { useWorkspace } from "../workspace/workspaceStore";
import { CommandPalette } from "./CommandPalette";
import { paletteCommands } from "./commands";
import { useUpdates } from "../updates/updateStore";
import { useSessionTables } from "./useSessionTables";

type Props = {
  profiles: readonly ConnectionProfile[];
  onClose: () => void;
  onNewConnection: () => void;
  onOpenConnection: (profile: ConnectionProfile) => void;
  onSettings: () => void;
  onShortcuts: () => void;
  /** A failure to report where the app shows action errors. */
  onError: (message: string) => void;
};

/** The palette fed with the app's state and actions. */
export function AppCommands(props: Props) {
  const { profiles, onClose, onError } = props;
  const { sessions, activeConnectionId } = useSessions();
  const savedQueries = useSavedQueries((s) => s.queries);
  const session = activeConnectionId ? (sessions[activeConnectionId] ?? null) : null;
  const tables = useSessionTables(session);

  const commands = useMemo(
    () =>
      paletteCommands(
        {
          profiles,
          sessions,
          activeConnectionId,
          tables,
          savedQueries: queriesFor(savedQueries, activeConnectionId),
        },
        {
          newConnection: props.onNewConnection,
          openSettings: props.onSettings,
          openShortcuts: props.onShortcuts,
          checkForUpdates: () => void useUpdates.getState().check("manual"),
          openConnection: props.onOpenConnection,
          newTab: (sessionId) => {
            useEditor.getState().openTab(sessionId);
          },
          refreshSchema: (sessionId) => void useSchema.getState().refresh(sessionId),
          disconnect: (connectionId) => {
            void disconnect(connectionId).then((problem) => {
              if (problem) onError(problem);
            });
          },
          openTable: (sessionId, table) => {
            useWorkspace.getState().openData(sessionId, table);
          },
          openSavedQuery: (query) => {
            void openSavedQuery(query, activeConnectionId).then((problem) => {
              if (problem) onError(problem);
            });
          },
        },
      ),
    // The handlers are recreated with the props on every render of App; the list need not be.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [profiles, sessions, activeConnectionId, tables, savedQueries],
  );

  return <CommandPalette commands={commands} onClose={onClose} />;
}
