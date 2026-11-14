import { useEffect, useState } from "react";

import { ConfirmDialog } from "../../components/ConfirmDialog";
import type { ConnectionProfile, SessionInfo } from "../../ipc";
import { EditorPane } from "../editor/EditorPane";
import { useCompletion } from "../editor/completionStore";
import { useEditor } from "../editor/editorStore";
import { loadTabs } from "../editor/persistence";
import { TableDetails } from "../schema/TableDetails";
import { matchShortcut } from "./shortcuts";
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
    void useCompletion.getState().load(session);
  }, [session]);
}

/** The main area for one session: editor tabs and table detail tabs. */
export function Workspace({ session, profile }: Props) {
  const { sessionId } = session;
  const workspace = useWorkspace((state) => workspaceOf(state, sessionId));
  const tabs = useEditor((state) => state.tabs);
  const [closing, setClosing] = useState<string | null>(null);
  useInitialTabs(session);

  const active = workspace.views.find((v) => v.key === workspace.active) ?? null;

  const requestClose = (view: View) => {
    if (view.kind === "table") {
      useWorkspace.getState().close(sessionId, view.key);
      return;
    }
    if (tabs[view.key]?.running) setClosing(view.key);
    else void useEditor.getState().closeTab(view.key);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const shortcut = matchShortcut(event);
      if (!shortcut) return;
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
        {active?.kind === "editor" && (
          <EditorPane key={active.key} tabId={active.key} session={session} profile={profile} />
        )}
        {!active && <p className="p-6 text-sm text-zinc-500">Open a new tab with ⌘T.</p>}
      </div>
      {closing && (
        <ConfirmDialog
          title="Close a running tab?"
          message="A statement is running in this tab. Closing it cancels the statement and closes the tab's connection."
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
