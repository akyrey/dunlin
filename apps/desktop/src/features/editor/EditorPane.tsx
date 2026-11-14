import { toast } from "../../components/toastStore";
import { useEffect, useMemo, useRef, useState } from "react";

import { Button } from "../../components/Button";
import { type ConnectionProfile, type SessionInfo, toAppError } from "../../ipc";
import type { Span } from "../../lib/text";
import { HistoryPanel } from "../history/HistoryPanel";
import { SaveQueryDialog } from "../saved/SaveQueryDialog";
import { useSavedQueries } from "../saved/savedQueriesStore";
import { DangerDialog } from "./DangerDialog";
import { ElapsedTimer } from "./ElapsedTimer";
import { ResultPanel } from "./ResultPanel";
import { RowLimitSelect } from "./RowLimitSelect";
import type { EditorHandle } from "./SqlEditor";
import { LazySqlEditor as SqlEditor } from "./lazy";
import { useCompletion } from "./completionStore";
import { type EditorTab, useEditor } from "./editorStore";
import { progress, type RunMode } from "./run";

/** How long typing pauses before the text is split again (task 5.5). */
const SPLIT_DELAY_MS = 250;

type Props = { tabId: string; session: SessionInfo; profile: ConnectionProfile | null };

/** The failing statement's error, underlined while the text is still what ran. */
function errorSpan(tab: EditorTab): Span | null {
  const failed = tab.run?.statements.find((s) => s.status === "failed");
  if (!failed || tab.text.slice(failed.start, failed.end) !== failed.sql) return null;
  if (failed.errorOffset === null) return { start: failed.start, end: failed.end };
  const rest = tab.text.slice(failed.errorOffset, failed.end);
  const word = /^\S+/.exec(rest)?.[0].length ?? 1;
  return { start: failed.errorOffset, end: Math.min(failed.end, failed.errorOffset + word) };
}

export function EditorPane({ tabId, session, profile }: Props) {
  const tab = useEditor((state) => state.tabs[tabId]);
  const completion = useCompletion((state) => state.completions[session.sessionId]);
  const actions = useEditor.getState();
  const editor = useRef<EditorHandle | null>(null);
  const [showHistory, setShowHistory] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const text = tab?.text;

  useEffect(() => {
    if (text === undefined) return;
    const timer = setTimeout(() => {
      useEditor
        .getState()
        .updateSplit(tabId)
        .catch(() => {
          // The highlight is a hint; running reports split errors where it matters.
        });
    }, SPLIT_DELAY_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [tabId, text]);

  const ranges = tab?.split?.text === tab?.text ? (tab?.split?.ranges ?? null) : null;
  const errorRange = useMemo(() => (tab ? errorSpan(tab) : null), [tab]);
  if (!tab) return null;

  const run = (mode: RunMode) => {
    const selection = editor.current?.selection() ?? { from: 0, to: 0, head: 0 };
    void actions.run(tabId, selection, mode);
  };
  const steps = tab.run ? progress(tab.run) : null;

  /** ⌘S: back to the tab's saved query, or a new one. */
  const save = async () => {
    setSaveError(null);
    const linked = tab.savedQueryId
      ? useSavedQueries.getState().queries.find((q) => q.id === tab.savedQueryId)
      : undefined;
    if (!linked) {
      setSaving(true);
      return;
    }
    try {
      await useSavedQueries.getState().save({ ...linked, sql: tab.text });
      toast.success(`Saved ${linked.name}`);
    } catch (caught) {
      setSaveError(toAppError(caught).message);
    }
  };

  return (
    <div className="flex h-full">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex flex-wrap items-center gap-2 border-b border-zinc-800 px-3 py-1.5">
          <Button
            variant="primary"
            disabled={tab.running}
            title="Run the selection or the statement under the cursor (⌘↵)"
            onClick={() => {
              run("statement");
            }}
          >
            {tab.running ? "Running…" : "Run"}
          </Button>
          <Button
            disabled={tab.running}
            title="Run every statement, stopping at the first error (⇧⌘↵)"
            onClick={() => {
              run("script");
            }}
          >
            Run script
          </Button>
          {tab.running && session.capabilities.cancel && (
            <Button
              variant="danger"
              title="Cancel (Esc)"
              onClick={() => void actions.cancel(tabId)}
            >
              Cancel
            </Button>
          )}
          {tab.running && steps && steps.total > 1 && (
            <span role="status" aria-label="Progress" className="text-xs text-zinc-400">
              {Math.min(steps.done + 1, steps.total)} / {steps.total}
            </span>
          )}
          {tab.running && tab.startedAt !== null && <ElapsedTimer startedAt={tab.startedAt} />}
          <span className="ml-auto flex items-center gap-3">
            <RowLimitSelect
              key={tab.rowLimit}
              value={tab.rowLimit}
              onChange={(limit) => {
                actions.setRowLimit(tabId, limit);
              }}
            />
            {saveError && (
              <span role="alert" className="text-xs text-rose-300">
                {saveError}
              </span>
            )}
            <Button
              variant="ghost"
              title="Save the SQL as a named query (⌘S)"
              onClick={() => void save()}
            >
              {tab.savedQueryId ? "Save" : "Save as…"}
            </Button>
            <Button
              variant="ghost"
              aria-pressed={showHistory}
              onClick={() => {
                setShowHistory((shown) => !shown);
              }}
            >
              History
            </Button>
          </span>
        </div>
        {completion?.error && (
          <p className="border-b border-zinc-800 px-3 py-0.5 text-[11px] text-amber-400">
            Autocomplete has keywords only: {completion.error.message}
          </p>
        )}
        <div className="min-h-0 flex-[3] border-b border-zinc-800">
          <SqlEditor
            key={`${tabId}:${String(tab.revision)}`}
            handleRef={editor}
            initialText={tab.text}
            driver={session.driver}
            completion={completion}
            ranges={ranges}
            errorRange={errorRange}
            running={tab.running}
            onChange={(next) => {
              actions.setText(tabId, next);
            }}
            onRun={(selection, mode) => void actions.run(tabId, selection, mode)}
            onCancel={() => void actions.cancel(tabId)}
            onSave={() => void save()}
          />
        </div>
        <section aria-label="Results" className="min-h-0 flex-[2]" tabIndex={-1} data-results>
          <ResultPanel
            tab={tab}
            onShow={(shown) => {
              actions.show(tabId, shown);
            }}
            onJumpTo={(offset) => editor.current?.focusAt(offset)}
          />
        </section>
      </div>
      {showHistory && profile && (
        <HistoryPanel
          connectionId={profile.id}
          onInsert={(statement) => {
            actions.insertText(tabId, statement);
          }}
          onOpen={(statement) => {
            actions.openTab(session.sessionId, { text: statement });
          }}
          onClose={() => {
            setShowHistory(false);
          }}
        />
      )}
      {saving && (
        <SaveQueryDialog
          initialName={tab.title}
          connectionName={profile?.name ?? "this connection"}
          onSave={async (name, everywhere) => {
            const saved = await useSavedQueries.getState().save({
              id: null,
              connectionId: everywhere ? null : (profile?.id ?? null),
              name,
              sql: tab.text,
            });
            actions.linkSaved(tabId, saved.id, saved.name);
            setSaving(false);
            toast.success(`Saved ${saved.name}`);
          }}
          onCancel={() => {
            setSaving(false);
          }}
        />
      )}
      {tab.confirmation && (
        <DangerDialog
          confirmation={tab.confirmation}
          connectionName={profile?.name ?? "this connection"}
          connectionColor={profile?.color ?? null}
          onConfirm={() => void actions.confirm(tabId)}
          onCancel={() => {
            actions.dismissConfirmation(tabId);
          }}
        />
      )}
    </div>
  );
}
