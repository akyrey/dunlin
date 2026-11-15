import { useEffect, useState } from "react";

import { ConfirmDialog } from "../../components/ConfirmDialog";
import type { ConnectionProfile, SessionInfo } from "../../ipc";
import { EditorPane } from "../editor/EditorPane";
import { useCompletion } from "../editor/completionStore";
import { useEditor } from "../editor/editorStore";
import { loadTabs } from "../editor/persistence";
import { TableData } from "../data/TableData";
import { useEdits } from "../edit/editStore";
import { TableDetails } from "../schema/TableDetails";
import { useSessions } from "../sessions/sessionsStore";
import { matchShortcut } from "../shortcuts/registry";
import { KeyView } from "../keys/KeyView";
import { TabStrip } from "./TabStrip";
import { type View, useWorkspace, workspaceOf } from "./workspaceStore";

type Props = { session: SessionInfo; profile: ConnectionProfile | null };

/** Opens the session's saved tabs, or one empty tab, the first time it is shown. */
function useInitialTabs(session: SessionInfo) {
  useEffect(() => {
    const { sessionId, connectionId } = session;
    if (workspaceOf(useWorkspace.getState(), sessionId).views.length > 0) return;
    const saved = loadTabs(connectionId);
    const { openTab } = useEditor.getState();
    if (saved.length === 0) openTab(sessionId);
    for (const tab of saved) openTab(sessionId, tab);
    const first = workspaceOf(useWorkspace.getState(), sessionId).views[0];
    if (first) useWorkspace.getState().activate(sessionId, first.key);
    // Key-value stores complete command names, not a schema.
    if (!session.capabilities.keyValue) void useCompletion.getState().load(session);
  }, [session]);
}

/** The main area for one session: editor tabs and table detail tabs. */
export function Workspace({ session, profile }: Props) {
  const { sessionId } = session;
  const workspace = useWorkspace((state) => workspaceOf(state, sessionId));
  const tabs = useEditor((state) => state.tabs);
  const [closing, setClosing] = useState<string | null>(null);
  /** A data view with unapplied edits waiting for "discard and close". */
  const [discarding, setDiscarding] = useState<string | null>(null);
  useInitialTabs(session);
  const unreachable = useSessions((s) => s.unreachable[sessionId] === true);

  const active = workspace.views.find((v) => v.key === workspace.active) ?? null;

  const requestClose = (view: View) => {
    if ((view.kind === "data" || view.kind === "editor") && useEdits.getState().pending[view.key]) {
      setDiscarding(view.key);
      return;
    }
    if (view.kind !== "editor") {
      useWorkspace.getState().close(sessionId, view.key);
      return;
    }
    if (tabs[view.key]?.running || tabs[view.key]?.transaction.open) setClosing(view.key);
    else void useEditor.getState().closeTab(view.key);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const shortcut = matchShortcut(event);
      // The palette, settings and the shortcut list are the app's (App.tsx).
      if (
        !shortcut ||
        shortcut === "palette" ||
        shortcut === "settings" ||
        shortcut === "shortcuts"
      )
        return;
      event.preventDefault();
      const { views, active: current } = workspaceOf(useWorkspace.getState(), sessionId);
      const view = views.find((v) => v.key === current);
      switch (shortcut) {
        case "new-tab":
          useEditor.getState().openTab(sessionId);
          return;
        case "close-tab":
          if (view) requestClose(view);
          return;
        case "next-tab":
          useWorkspace.getState().cycle(sessionId, 1);
          return;
        case "previous-tab":
          useWorkspace.getState().cycle(sessionId, -1);
          return;
        case "focus-results":
          document.querySelector<HTMLElement>("[data-results]")?.focus();
          return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  });

  return (
    <div className="flex h-full flex-col">
      {unreachable && (
        <p
          role="alert"
          className="border-b border-amber-900 bg-amber-950/60 px-3 py-1 text-xs text-amber-300"
        >
          The server is not answering. Dunlin reconnects by itself when it is back; statements are
          never re-run.
        </p>
      )}
      <TabStrip
        sessionId={sessionId}
        views={workspace.views}
        active={workspace.active}
        tabs={tabs}
        color={profile?.color ?? null}
        onClose={requestClose}
      />
      <div className="min-h-0 flex-1">
        {active?.kind === "table" && (
          <TableDetails
            key={`${sessionId}:${active.key}`}
            sessionId={sessionId}
            table={active.table}
            onOpenTable={(table) => {
              useWorkspace.getState().openTable(sessionId, table);
            }}
          />
        )}
        {active?.kind === "key" && (
          <KeyView
            key={`${sessionId}:${active.key}`}
            sessionId={sessionId}
            id={active.id}
            name={active.name}
          />
        )}
        {active?.kind === "editor" && (
          <EditorPane key={active.key} tabId={active.key} session={session} profile={profile} />
        )}
        {/* Data views stay mounted while hidden, so their page, filter and connection survive
            switching tabs. */}
        {workspace.views.map((view) =>
          view.kind === "data" ? (
            <div key={view.key} className="h-full" hidden={view.key !== workspace.active}>
              <TableData
                sessionId={sessionId}
                connectionId={session.connectionId}
                table={view.table}
                match={view.match}
                onFollow={(table, match) => {
                  useWorkspace.getState().openData(sessionId, table, match);
                }}
                viewKey={view.key}
                readOnly={session.readOnly}
                profile={{
                  name: profile?.name ?? "this connection",
                  color: profile?.color ?? null,
                  alwaysPreview: profile?.alwaysPreview ?? false,
                }}
              />
            </div>
          ) : null,
        )}
        {!active && <p className="p-6 text-sm text-zinc-500">Open a new tab with ⌘T.</p>}
      </div>
      {discarding && (
        <ConfirmDialog
          title="Discard unapplied changes?"
          message="This tab has edits that were not applied. Closing it discards them; nothing was written to the database."
          confirmLabel="Discard and close"
          onConfirm={() => {
            if (tabs[discarding]) void useEditor.getState().closeTab(discarding);
            else useWorkspace.getState().close(sessionId, discarding);
            setDiscarding(null);
          }}
          onCancel={() => {
            setDiscarding(null);
          }}
        />
      )}
      {closing && (
        <ConfirmDialog
          title={tabs[closing]?.running ? "Close a running tab?" : "Roll back and close?"}
          message={
            tabs[closing]?.running
              ? "A statement is running in this tab. Closing it cancels the statement and closes the tab's connection."
              : "This tab has an open transaction. Closing it rolls the transaction back; nothing in it is kept."
          }
          confirmLabel="Close tab"
          onConfirm={() => {
            const tabId = closing;
            setClosing(null);
            void useEditor
              .getState()
              .cancel(tabId)
              .finally(() => useEditor.getState().closeTab(tabId));
          }}
          onCancel={() => {
            setClosing(null);
          }}
        />
      )}
    </div>
  );
}
