import type { SessionInfo } from "../../ipc";
import { EditorPane } from "../editor/EditorPane";
import { TableDetails } from "../schema/TableDetails";
import { EDITOR_VIEW, useWorkspace, workspaceOf } from "./workspaceStore";

/** The main area for one session: the SQL editor plus one tab per opened table. */
export function Workspace({ session }: { session: SessionInfo }) {
  const { sessionId } = session;
  const workspace = useWorkspace((state) => workspaceOf(state, sessionId));
  const { activate, close, openTable } = useWorkspace.getState();
  const active = workspace.details.find((d) => d.key === workspace.active);

  return (
    <div className="flex h-full flex-col">
      {workspace.details.length > 0 && (
        <div
          role="tablist"
          aria-label="Open views"
          className="flex shrink-0 overflow-x-auto border-b border-zinc-800 bg-zinc-900"
        >
          <ViewTab
            label="SQL"
            selected={!active}
            onSelect={() => {
              activate(sessionId, EDITOR_VIEW);
            }}
          />
          {workspace.details.map((detail) => (
            <ViewTab
              key={detail.key}
              label={detail.table.name}
              selected={detail.key === workspace.active}
              onSelect={() => {
                activate(sessionId, detail.key);
              }}
              onClose={() => {
                close(sessionId, detail.key);
              }}
            />
          ))}
        </div>
      )}
      <div className="min-h-0 flex-1">
        {active ? (
          <TableDetails
            key={`${sessionId}:${active.key}`}
            sessionId={sessionId}
            table={active.table}
            onOpenTable={(table) => {
              openTable(sessionId, table);
            }}
          />
        ) : (
          <EditorPane session={session} />
        )}
      </div>
    </div>
  );
}

function ViewTab({
  label,
  selected,
  onSelect,
  onClose,
}: {
  label: string;
  selected: boolean;
  onSelect: () => void;
  onClose?: () => void;
}) {
  return (
    <div
      className={`flex items-center border-r border-zinc-800 text-sm ${selected ? "bg-zinc-950 text-zinc-100" : "text-zinc-400 hover:text-zinc-200"}`}
    >
      <button
        type="button"
        role="tab"
        aria-selected={selected}
        className="max-w-48 truncate px-3 py-1.5"
        onClick={onSelect}
      >
        {label}
      </button>
      {onClose && (
        <button
          type="button"
          aria-label={`Close ${label}`}
          className="pr-2 text-xs text-zinc-500 hover:text-zinc-100"
          onClick={onClose}
        >
          ×
        </button>
      )}
    </div>
  );
}
