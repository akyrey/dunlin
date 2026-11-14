import type { ReactNode } from "react";

import type { AppError, QueryResult } from "../../ipc";
import { lineAndColumn } from "../../lib/text";
import { ResultView } from "../grid/ResultView";
import type { EditorTab } from "./editorStore";
import type { StatementRun, StatementStatus } from "./run";

type Props = {
  tab: EditorTab;
  onShow: (shown: number | "messages") => void;
  onJumpTo: (offset: number) => void;
};

const STATUS_MARK: Record<StatementStatus, string> = {
  pending: "·",
  running: "…",
  done: "",
  failed: "✕",
  cancelled: "⊘",
  skipped: "–",
};

/** One sub-tab per statement of the last run, plus the run's Messages log (task 5.7). */
export function ResultPanel({ tab, onShow, onJumpTo }: Props) {
  if (tab.notice)
    return <ErrorView error={tab.notice} offset={null} text={tab.text} onJumpTo={onJumpTo} />;
  const run = tab.run;
  if (!run)
    return <Message>Run a statement with ⌘↵ (Ctrl+Enter), or the whole script with ⇧⌘↵.</Message>;
  const shown =
    tab.shown === "messages" ? "messages" : Math.min(tab.shown, run.statements.length - 1);
  const statement = shown === "messages" ? null : run.statements[shown];
  return (
    <div className="flex h-full flex-col">
      <div
        role="tablist"
        aria-label="Results"
        className="flex shrink-0 overflow-x-auto border-b border-zinc-800"
      >
        {run.statements.map((s, i) => (
          <SubTab
            key={i}
            selected={shown === i}
            label={`Result ${i + 1}`}
            mark={STATUS_MARK[s.status]}
            failed={s.status === "failed"}
            onSelect={() => {
              onShow(i);
            }}
          />
        ))}
        <SubTab
          selected={shown === "messages"}
          label="Messages"
          mark=""
          failed={false}
          onSelect={() => {
            onShow("messages");
          }}
        />
      </div>
      <div role="tabpanel" className="min-h-0 flex-1">
        {statement ? (
          <StatementView statement={statement} text={tab.text} onJumpTo={onJumpTo} />
        ) : (
          <Messages run={run} />
        )}
      </div>
    </div>
  );
}

function SubTab(props: {
  selected: boolean;
  label: string;
  mark: string;
  failed: boolean;
  onSelect: () => void;
}) {
  const { selected, label, mark, failed, onSelect } = props;
  return (
    <button
      type="button"
      role="tab"
      aria-selected={selected}
      className={`shrink-0 px-3 py-1 text-xs ${selected ? "border-b-2 border-sky-500 text-zinc-100" : "text-zinc-400 hover:text-zinc-200"} ${failed ? "text-rose-300" : ""}`}
      onClick={onSelect}
    >
      {label}
      {mark && <span aria-hidden="true"> {mark}</span>}
    </button>
  );
}

function StatementView({
  statement,
  text,
  onJumpTo,
}: {
  statement: StatementRun;
  text: string;
  onJumpTo: (offset: number) => void;
}) {
  return (
    <div className="flex h-full flex-col">
      <p
        className="truncate border-b border-zinc-800/70 px-3 py-1 font-mono text-[11px] text-zinc-500"
        title={statement.sql}
      >
        {statement.sql}
      </p>
      <div className="min-h-0 flex-1">
        <StatementBody statement={statement} text={text} onJumpTo={onJumpTo} />
      </div>
    </div>
  );
}

function StatementBody({
  statement,
  text,
  onJumpTo,
}: {
  statement: StatementRun;
  text: string;
  onJumpTo: (offset: number) => void;
}) {
  switch (statement.status) {
    case "pending":
      return <Message>Waiting for the statements before it.</Message>;
    case "running":
      return <Message>Running…</Message>;
    case "skipped":
      return <Message>Not run: an earlier statement failed.</Message>;
    case "cancelled":
      return statement.error ? (
        <ErrorView error={statement.error} offset={null} text={text} onJumpTo={onJumpTo} />
      ) : (
        <Message>Not run: the script was cancelled.</Message>
      );
    case "failed":
      return statement.error ? (
        <ErrorView
          error={statement.error}
          offset={statement.errorOffset}
          text={text}
          onJumpTo={onJumpTo}
        />
      ) : null;
    case "done":
      return statement.result ? <StatementResult result={statement.result} /> : null;
  }
}

function StatementResult({ result }: { result: QueryResult }) {
  const { outcome } = result;
  if (outcome.kind === "affected") {
    const noun = outcome.rows === "1" ? "row" : "rows";
    return <Message>{`${outcome.rows} ${noun} affected · ${result.elapsedMs} ms`}</Message>;
  }
  return (
    <div className="flex h-full flex-col">
      {outcome.truncated && (
        <p
          role="status"
          className="border-b border-amber-900 bg-amber-950/60 px-3 py-1 text-xs text-amber-300"
        >
          Showing the first {outcome.rows.length} rows; more are available. Raise the row limit to
          see them.
        </p>
      )}
      <div className="min-h-0 flex-1">
        <ResultView
          columns={outcome.columns}
          rows={outcome.rows}
          loadFull={null}
          noFullReason="Results of a statement in the editor cannot be fetched again by row; open the table from the tree to load whole values."
        />
      </div>
    </div>
  );
}

function Messages({ run }: { run: EditorTab["run"] }) {
  return (
    <ol aria-label="Messages" className="h-full overflow-auto px-3 py-2 font-mono text-xs">
      {run?.messages.map((message, i) => (
        <li key={i} className={message.level === "error" ? "text-rose-300" : "text-zinc-400"}>
          <time className="mr-2 text-zinc-600">{new Date(message.at).toLocaleTimeString()}</time>
          {message.text}
        </li>
      ))}
    </ol>
  );
}

function Message({ children }: { children: ReactNode }) {
  return (
    <p role="status" className="px-3 py-2 text-sm text-zinc-400">
      {children}
    </p>
  );
}

function ErrorView({
  error,
  offset,
  text,
  onJumpTo,
}: {
  error: AppError;
  offset: number | null;
  text: string;
  onJumpTo: (offset: number) => void;
}) {
  const location = offset === null ? null : lineAndColumn(text, offset);
  return (
    <div role="alert" className="px-3 py-2 text-sm">
      <p className="font-mono text-rose-300">{error.message}</p>
      <p className="mt-1 text-xs text-zinc-500">
        {error.code}
        {location && offset !== null && (
          <>
            {" · "}
            <button
              type="button"
              className="underline hover:text-zinc-300"
              onClick={() => {
                onJumpTo(offset);
              }}
            >
              line {location.line}, column {location.column}
            </button>
          </>
        )}
      </p>
    </div>
  );
}
