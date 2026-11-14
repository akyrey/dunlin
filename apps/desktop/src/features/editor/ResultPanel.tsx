import { ResultGrid } from "../grid/ResultGrid";
import { lineAndColumn } from "../../lib/text";
import type { QueryError, TabState } from "./editorStore";

type Props = {
  tab: TabState;
  onJumpTo: (offset: number) => void;
};

export function ResultPanel({ tab, onJumpTo }: Props) {
  if (tab.error) return <ErrorPanel error={tab.error} text={tab.text} onJumpTo={onJumpTo} />;
  if (tab.running && !tab.result) return <Message>Running…</Message>;
  if (!tab.result)
    return <Message>Run a statement with ⌘↵ (Ctrl+Enter) or the Run button.</Message>;
  const { outcome } = tab.result;
  if (outcome.kind === "affected") {
    const noun = outcome.rows === "1" ? "row" : "rows";
    return <Message>{`${outcome.rows} ${noun} affected.`}</Message>;
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
        <ResultGrid columns={outcome.columns} rows={outcome.rows} />
      </div>
    </div>
  );
}

function Message({ children }: { children: React.ReactNode }) {
  return (
    <p role="status" className="px-3 py-2 text-sm text-zinc-400">
      {children}
    </p>
  );
}

function ErrorPanel({
  error,
  text,
  onJumpTo,
}: {
  error: QueryError;
  text: string;
  onJumpTo: (offset: number) => void;
}) {
  const { offset } = error;
  const location = offset === null ? null : lineAndColumn(text, offset);
  return (
    <div role="alert" className="px-3 py-2 text-sm">
      <p className="font-mono text-rose-300">{error.error.message}</p>
      <p className="mt-1 text-xs text-zinc-500">
        {error.error.code}
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
