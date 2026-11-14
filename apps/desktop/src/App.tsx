import { useEffect, useState } from "react";

import { Button } from "./components/Button";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { StatusBar } from "./components/StatusBar";
import { ConnectionForm } from "./features/connections/ConnectionForm";
import { ConnectionList } from "./features/connections/ConnectionList";
import { useConnections } from "./features/connections/connectionsStore";
import { draftFromProfile, emptyDraft } from "./features/connections/draft";
import { EditorPane } from "./features/editor/EditorPane";
import { useEditor } from "./features/editor/editorStore";
import { useSessions } from "./features/sessions/sessionsStore";
import { type AppError, type ConnectionDraft, type ConnectionProfile, toAppError } from "./ipc";

export function App() {
  const { drivers, profiles, error: loadError } = useConnections();
  const { sessions, activeConnectionId } = useSessions();
  const [editing, setEditing] = useState<ConnectionDraft | null>(null);
  const [deleting, setDeleting] = useState<ConnectionProfile | null>(null);
  const [actionError, setActionError] = useState<AppError | null>(null);

  useEffect(() => {
    void useConnections.getState().load();
  }, []);

  const activeProfile = profiles.find((p) => p.id === activeConnectionId) ?? null;
  const activeSession = activeConnectionId ? (sessions[activeConnectionId] ?? null) : null;
  const activeTab = useEditor((state) =>
    activeSession ? state.tabs[activeSession.sessionId] : undefined,
  );
  const editingDriver = editing ? drivers.find((d) => d.kind === editing.driver) : undefined;
  const firstDriver = drivers[0];

  const openConnection = async (profile: ConnectionProfile) => {
    setActionError(null);
    try {
      await useSessions.getState().open(profile.id);
    } catch (error) {
      setActionError(toAppError(error));
    }
  };

  const saveConnection = async (draft: ConnectionDraft) => {
    await useConnections.getState().save(draft);
    setEditing(null);
  };

  const deleteConnection = async (profile: ConnectionProfile) => {
    setDeleting(null);
    try {
      const session = useSessions.getState().sessions[profile.id];
      await useConnections.getState().remove(profile.id);
      useSessions.getState().forget(profile.id);
      if (session) useEditor.getState().dropTab(session.sessionId);
    } catch (error) {
      setActionError(toAppError(error));
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-64 shrink-0 flex-col border-r border-zinc-800 bg-zinc-900">
          <div className="flex items-center justify-between px-3 py-2">
            <h1 className="text-sm font-semibold tracking-wide">Dunlin</h1>
            <Button
              variant="ghost"
              disabled={!firstDriver}
              onClick={() => {
                if (firstDriver) setEditing(emptyDraft(firstDriver));
              }}
            >
              New connection
            </Button>
          </div>
          <ConnectionList
            profiles={profiles}
            openIds={new Set(Object.keys(sessions))}
            activeId={activeConnectionId}
            onOpen={(profile) => void openConnection(profile)}
            onEdit={(profile) => {
              setEditing(draftFromProfile(profile));
            }}
            onDelete={setDeleting}
          />
          {(loadError ?? actionError) && (
            <p
              role="alert"
              className="mx-3 mt-2 rounded bg-rose-950/60 px-2 py-1 text-xs text-rose-300"
            >
              {(loadError ?? actionError)?.message}
            </p>
          )}
        </aside>
        <main className="min-w-0 flex-1">
          {activeSession ? (
            <EditorPane session={activeSession} />
          ) : (
            <p className="p-6 text-sm text-zinc-500">Open a connection to start writing SQL.</p>
          )}
        </main>
      </div>
      <StatusBar
        profile={activeProfile}
        session={activeSession}
        result={activeTab?.result ?? null}
      />
      {editing && editingDriver && (
        <ConnectionForm
          initial={editing}
          driver={editingDriver}
          onSave={saveConnection}
          onCancel={() => {
            setEditing(null);
          }}
        />
      )}
      {deleting && (
        <ConfirmDialog
          title="Delete connection?"
          message={`"${deleting.name}" will be removed. The database itself is not touched.`}
          confirmLabel="Delete"
          onConfirm={() => void deleteConnection(deleting)}
          onCancel={() => {
            setDeleting(null);
          }}
        />
      )}
    </div>
  );
}
