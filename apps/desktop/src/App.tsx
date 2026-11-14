import { useEffect, useMemo, useState } from "react";

import { Button } from "./components/Button";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { StatusBar } from "./components/StatusBar";
import { ConnectionForm } from "./features/connections/ConnectionForm";
import { ConnectionList } from "./features/connections/ConnectionList";
import { useConnections } from "./features/connections/connectionsStore";
import { draftFromProfile, emptyDraft } from "./features/connections/draft";
import { useEditor } from "./features/editor/editorStore";
import { SchemaTree } from "./features/schema/SchemaTree";
import { useSchema } from "./features/schema/schemaStore";
import { useSessions } from "./features/sessions/sessionsStore";
import { HostKeyDialog } from "./features/ssh/HostKeyDialog";
import { TrustedHosts } from "./features/ssh/TrustedHosts";
import { describeConnectError, hostKeyFrom } from "./features/ssh/hostKey";
import { Workspace } from "./features/workspace/Workspace";
import { useWorkspace } from "./features/workspace/workspaceStore";
import {
  type AppError,
  type ConnectionDraft,
  type ConnectionProfile,
  type ConnectionSecrets,
  type SshTrustHostArgs,
  sshTrustHost,
  toAppError,
} from "./ipc";

type HostKeyPrompt = { profile: ConnectionProfile; hostKey: SshTrustHostArgs; changed: boolean };

export function App() {
  const { drivers, profiles, error: loadError } = useConnections();
  const { sessions, activeConnectionId, notice } = useSessions();
  const [hostKeyPrompt, setHostKeyPrompt] = useState<HostKeyPrompt | null>(null);
  const [showTrustedHosts, setShowTrustedHosts] = useState(false);
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
  const editingSavedSecrets = profiles.find((p) => p.id === editing?.id)?.savedSecrets ?? [];
  const firstDriver = drivers[0];
  const treeSessions = useMemo(
    () =>
      profiles.flatMap((profile) => {
        const session = sessions[profile.id];
        return session
          ? [{ connectionId: profile.id, sessionId: session.sessionId, label: profile.name }]
          : [];
      }),
    [profiles, sessions],
  );

  // Forget the tree and tabs of sessions that ended (closed, deleted or lost).
  useEffect(() => {
    const open = new Set(Object.values(sessions).map((s) => s.sessionId));
    for (const store of [useSchema, useWorkspace]) {
      const known = Object.keys(
        store === useSchema ? useSchema.getState().trees : useWorkspace.getState().workspaces,
      );
      for (const sessionId of known) {
        if (!open.has(sessionId)) store.getState().dropSession(sessionId);
      }
    }
  }, [sessions]);

  const openConnection = async (profile: ConnectionProfile) => {
    setActionError(null);
    try {
      await useSessions.getState().open(profile.id);
    } catch (caught) {
      const error = toAppError(caught);
      const hostKey = error.code.startsWith("ssh_host_key_") ? hostKeyFrom(error) : null;
      if (hostKey) {
        setHostKeyPrompt({ profile, hostKey, changed: error.code === "ssh_host_key_changed" });
        return;
      }
      setActionError(error);
    }
  };

  const trustAndOpen = async (prompt: HostKeyPrompt) => {
    setHostKeyPrompt(null);
    try {
      await sshTrustHost(prompt.hostKey);
    } catch (error) {
      setActionError(toAppError(error));
      return;
    }
    await openConnection(prompt.profile);
  };

  const saveConnection = async (draft: ConnectionDraft, secrets?: ConnectionSecrets) => {
    await useConnections.getState().save(draft, secrets);
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
              {loadError ? loadError.message : actionError && describeConnectError(actionError)}
            </p>
          )}
          <SchemaTree sessions={treeSessions} />
          {notice && (
            <p
              role="alert"
              className="mx-3 mt-2 rounded bg-amber-950/60 px-2 py-1 text-xs text-amber-300"
            >
              {notice}
            </p>
          )}
          <div className="mt-auto border-t border-zinc-800 px-3 py-2">
            <Button
              variant="ghost"
              onClick={() => {
                setShowTrustedHosts(true);
              }}
            >
              Trusted SSH hosts
            </Button>
          </div>
        </aside>
        <main className="min-w-0 flex-1">
          {activeSession ? (
            <Workspace session={activeSession} />
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
      {editing && (
        <ConnectionForm
          initial={editing}
          drivers={drivers}
          savedSecrets={editingSavedSecrets}
          onSave={saveConnection}
          onCancel={() => {
            setEditing(null);
          }}
        />
      )}
      {hostKeyPrompt && (
        <HostKeyDialog
          hostKey={hostKeyPrompt.hostKey}
          changed={hostKeyPrompt.changed}
          onTrust={() => void trustAndOpen(hostKeyPrompt)}
          onCancel={() => {
            setHostKeyPrompt(null);
          }}
        />
      )}
      {showTrustedHosts && (
        <TrustedHosts
          onClose={() => {
            setShowTrustedHosts(false);
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
