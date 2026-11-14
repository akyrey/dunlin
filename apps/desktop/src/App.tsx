import { useEffect, useMemo, useState } from "react";

import { Button } from "./components/Button";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { PanelBoundary } from "./components/PanelBoundary";
import { StatusBar } from "./components/StatusBar";
import { Toaster } from "./components/Toaster";
import { toast } from "./components/toastStore";
import { ConnectionForm } from "./features/connections/ConnectionForm";
import { ConnectionList } from "./features/connections/ConnectionList";
import { useConnections } from "./features/connections/connectionsStore";
import { pendingViews, useEdits } from "./features/edit/editStore";
import { QuitGuard } from "./features/edit/quitGuard";
import { draftFromProfile, emptyDraft } from "./features/connections/draft";
import { shownResult, useEditor } from "./features/editor/editorStore";
import { persistTabs } from "./features/editor/persistence";
import { SavedQueries } from "./features/saved/SavedQueries";
import { useSavedQueries } from "./features/saved/savedQueriesStore";
import { SchemaTree } from "./features/schema/SchemaTree";
import { useSessions } from "./features/sessions/sessionsStore";
import { HostKeyDialog } from "./features/ssh/HostKeyDialog";
import { AppCommands } from "./features/palette/AppCommands";
import { SettingsView } from "./features/settings/SettingsView";
import { ShortcutsView } from "./features/shortcuts/ShortcutsView";
import { matchShortcut, shortcutLabel } from "./features/shortcuts/registry";
import { useSettings } from "./features/settings/settingsStore";
import { useApplyTheme } from "./features/settings/theme";
import { TrustedHosts } from "./features/ssh/TrustedHosts";
import { describeConnectError, hostKeyFrom } from "./features/ssh/hostKey";
import { SidebarResizer } from "./features/workspace/SidebarResizer";
import { Workspace } from "./features/workspace/Workspace";
import { useLayoutRestore } from "./features/workspace/useLayoutRestore";
import { forgetEndedSessions } from "./features/workspace/lifecycle";
import { useWorkspace, workspaceOf } from "./features/workspace/workspaceStore";
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

/** Deleting a connection closes its session, which loses any unapplied edits. */
function unappliedNote(profile: ConnectionProfile): string {
  const session = useSessions.getState().sessions[profile.id];
  if (!session) return "";
  const count = pendingViews(useEdits.getState(), session.sessionId).length;
  return count > 0
    ? ` ${count} ${count === 1 ? "table has" : "tables have"} unapplied changes, which will be lost.`
    : "";
}

export function App() {
  const { drivers, profiles, loaded, error: loadError } = useConnections();
  const { sessions, activeConnectionId, notice, remembered } = useSessions();
  const [hostKeyPrompt, setHostKeyPrompt] = useState<HostKeyPrompt | null>(null);
  const [showTrustedHosts, setShowTrustedHosts] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [overlay, setOverlay] = useState<"palette" | "shortcuts" | null>(null);
  useApplyTheme();
  const [editing, setEditing] = useState<ConnectionDraft | null>(null);
  const [deleting, setDeleting] = useState<ConnectionProfile | null>(null);
  const [actionError, setActionError] = useState<AppError | null>(null);

  useEffect(() => {
    void useConnections.getState().load();
    void useSavedQueries.getState().load();
    void useSettings.getState().load();
  }, []);

  const activeProfile = profiles.find((p) => p.id === activeConnectionId) ?? null;
  const activeSession = activeConnectionId ? (sessions[activeConnectionId] ?? null) : null;
  const activeView = useWorkspace((state) =>
    activeSession ? workspaceOf(state, activeSession.sessionId).active : null,
  );
  const activeResult = useEditor((state) =>
    shownResult(activeView ? state.tabs[activeView] : undefined),
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

  // Forget the tree, tabs and completion of sessions that ended (closed, deleted or lost).
  useEffect(() => {
    forgetEndedSessions(new Set(Object.values(sessions).map((s) => s.sessionId)));
  }, [sessions]);

  // Editor tabs are saved locally and restored when the connection is opened again.
  useEffect(
    () =>
      persistTabs(() =>
        Object.values(useSessions.getState().sessions).map(({ sessionId, connectionId }) => ({
          sessionId,
          connectionId,
        })),
      ),
    [],
  );

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const shortcut = matchShortcut(event);
      if (shortcut !== "palette" && shortcut !== "settings" && shortcut !== "shortcuts") return;
      event.preventDefault();
      if (shortcut === "settings") {
        setOverlay(null);
        setShowSettings(true);
      } else setOverlay((current) => (current === shortcut ? null : shortcut));
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, []);

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

  const layout = useLayoutRestore(profiles, loaded, openConnection);

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
      await useConnections.getState().remove(profile.id);
      useSessions.getState().forget(profile.id);
    } catch (error) {
      setActionError(toAppError(error));
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <aside
          className="flex shrink-0 flex-col bg-zinc-900"
          style={{ width: layout.sidebarWidth }}
        >
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
            rememberedIds={new Set(remembered)}
            activeId={activeConnectionId}
            onOpen={(profile) => void openConnection(profile)}
            onEdit={(profile) => {
              setEditing(draftFromProfile(profile));
            }}
            onDelete={setDeleting}
            onDuplicate={(profile) => {
              useConnections
                .getState()
                .duplicate(profile.id)
                .then((copy) => toast.success(`Created ${copy.name}`))
                .catch((caught: unknown) => {
                  toast.error(toAppError(caught).message);
                });
            }}
            onMove={(profile, step) => {
              useConnections
                .getState()
                .move(profile.id, step)
                .catch((caught: unknown) => {
                  toast.error(toAppError(caught).message);
                });
            }}
          />
          {(loadError ?? actionError) && (
            <p
              role="alert"
              className="mx-3 mt-2 rounded bg-rose-950/60 px-2 py-1 text-xs text-rose-300"
            >
              {loadError ? loadError.message : actionError && describeConnectError(actionError)}
            </p>
          )}
          <PanelBoundary name="schema tree">
            <SchemaTree sessions={treeSessions} />
          </PanelBoundary>
          <PanelBoundary name="saved queries list">
            <SavedQueries activeConnectionId={activeConnectionId} />
          </PanelBoundary>
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
                setShowSettings(true);
              }}
            >
              Settings
            </Button>
          </div>
        </aside>
        <SidebarResizer width={layout.sidebarWidth} onResize={layout.setSidebarWidth} />
        <main className="min-w-0 flex-1">
          {activeSession ? (
            <PanelBoundary key={activeSession.sessionId} name="workspace">
              <Workspace session={activeSession} profile={activeProfile} />
            </PanelBoundary>
          ) : (
            <p className="p-6 text-sm text-zinc-500">
              Open a connection to start writing SQL, or press {shortcutLabel("palette")} for
              everything else.
            </p>
          )}
        </main>
      </div>
      <StatusBar profile={activeProfile} session={activeSession} result={activeResult} />
      {editing && (
        <ConnectionForm
          initial={editing}
          drivers={drivers}
          savedSecrets={editingSavedSecrets}
          folders={[...new Set(profiles.flatMap((p) => (p.folder ? [p.folder] : [])))]}
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
      {overlay === "palette" && (
        <AppCommands
          profiles={profiles}
          onClose={() => {
            setOverlay(null);
          }}
          onNewConnection={() => {
            if (firstDriver) setEditing(emptyDraft(firstDriver));
          }}
          onOpenConnection={(profile) => void openConnection(profile)}
          onSettings={() => {
            setShowSettings(true);
          }}
          onShortcuts={() => {
            setOverlay("shortcuts");
          }}
          onError={(message) => {
            toast.error(message);
          }}
        />
      )}
      {overlay === "shortcuts" && (
        <ShortcutsView
          onClose={() => {
            setOverlay(null);
          }}
        />
      )}
      {showSettings && (
        <SettingsView
          onClose={() => {
            setShowSettings(false);
          }}
          onShowTrustedHosts={() => {
            setShowSettings(false);
            setShowTrustedHosts(true);
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
      <QuitGuard />
      <Toaster />
      {deleting && (
        <ConfirmDialog
          title="Delete connection?"
          message={`"${deleting.name}" will be removed. The database itself is not touched.${unappliedNote(deleting)}`}
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
