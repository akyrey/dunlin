import { useEffect, useRef } from "react";

import { Button } from "../../components/Button";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import type { SessionInfo } from "../../ipc";
import { ElapsedTimer } from "./ElapsedTimer";
import { ResultPanel } from "./ResultPanel";
import { type EditorHandle, SqlEditor } from "./SqlEditor";
import { useEditor } from "./editorStore";

export function EditorPane({ session }: { session: SessionInfo }) {
  const { sessionId } = session;
  const tab = useEditor((state) => state.tabs[sessionId]);
  const { ensureTab, setText, run, confirm, cancel, dismissConfirmation } = useEditor.getState();
  const editor = useRef<EditorHandle | null>(null);

  useEffect(() => {
    ensureTab(sessionId);
  }, [ensureTab, sessionId]);

  if (!tab) return null;

  const runCurrent = () => {
    const selection = editor.current?.selection() ?? { from: 0, to: 0, head: 0 };
    void run(sessionId, selection);
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-zinc-800 px-3 py-1.5">
        <Button variant="primary" disabled={tab.running} onClick={runCurrent}>
          {tab.running ? "Running…" : "Run"}
        </Button>
        {tab.running && (
          <>
            {session.capabilities.cancel && (
              <Button variant="danger" onClick={() => void cancel(sessionId)}>
                Cancel
              </Button>
            )}
            {tab.startedAt !== null && <ElapsedTimer startedAt={tab.startedAt} />}
          </>
        )}
        <span className="text-xs text-zinc-500">
          ⌘↵ runs the selection or the statement under the cursor
        </span>
      </div>
      <div className="min-h-0 flex-[3] border-b border-zinc-800">
        <SqlEditor
          key={sessionId}
          handleRef={editor}
          initialText={tab.text}
          onChange={(text) => {
            setText(sessionId, text);
          }}
          onRun={(selection) => void run(sessionId, selection)}
        />
      </div>
      <section aria-label="Results" className="min-h-0 flex-[2]">
        <ResultPanel tab={tab} onJumpTo={(offset) => editor.current?.focusAt(offset)} />
      </section>
      {tab.confirmation && (
        <ConfirmDialog
          title="Run a dangerous statement?"
          message={tab.confirmation.reason}
          detail={tab.confirmation.sql}
          confirmLabel="Run anyway"
          onConfirm={() => void confirm(sessionId)}
          onCancel={() => {
            dismissConfirmation(sessionId);
          }}
        />
      )}
    </div>
  );
}
