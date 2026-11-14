import { useState } from "react";

import type { EditorTab } from "../editor/editorStore";
import { useEditor } from "../editor/editorStore";
import { SHORTCUT_LABELS } from "./shortcuts";
import { type View, useWorkspace } from "./workspaceStore";

type Props = {
  sessionId: string;
  views: View[];
  active: string | null;
  tabs: Record<string, EditorTab>;
  /** The connection's colour tag, shown on every tab (FR-6.3). */
  color: string | null;
  onClose: (view: View) => void;
};

const DRAG_TYPE = "application/x-dunlin-tab";

/** Editor and table tabs: select, close, rename (double-click), reorder (drag), new. */
export function TabStrip({ sessionId, views, active, tabs, color, onClose }: Props) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const label = (view: View) =>
    view.kind === "editor" ? (tabs[view.key]?.title ?? "") : view.table.name;

  return (
    <div className="flex shrink-0 items-stretch border-b border-zinc-800 bg-zinc-900">
      <div role="tablist" aria-label="Open tabs" className="flex min-w-0 overflow-x-auto">
        {views.map((view, index) => (
          <div
            key={view.key}
            draggable
            onDragStart={(e) => {
              e.dataTransfer.setData(DRAG_TYPE, view.key);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragOver={(e) => {
              if (e.dataTransfer.types.includes(DRAG_TYPE)) e.preventDefault();
            }}
            onDrop={(e) => {
              const key = e.dataTransfer.getData(DRAG_TYPE);
              if (key) useWorkspace.getState().move(sessionId, key, index);
            }}
            className={`flex items-center border-r border-zinc-800 text-sm ${view.key === active ? "bg-zinc-950 text-zinc-100" : "text-zinc-400 hover:text-zinc-200"}`}
            style={
              view.kind === "editor"
                ? { boxShadow: `inset 0 2px 0 ${color ?? "transparent"}` }
                : undefined
            }
          >
            {renaming === view.key ? (
              <input
                aria-label="Tab name"
                autoFocus
                defaultValue={label(view)}
                className="mx-1 w-28 rounded bg-zinc-800 px-1 text-sm"
                onBlur={(e) => {
                  useEditor.getState().rename(view.key, e.target.value);
                  setRenaming(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") e.currentTarget.blur();
                  if (e.key === "Escape") setRenaming(null);
                }}
              />
            ) : (
              <button
                type="button"
                role="tab"
                aria-selected={view.key === active}
                className="max-w-48 truncate px-3 py-1.5"
                title={
                  view.kind === "editor" ? "Double-click to rename; drag to reorder" : undefined
                }
                onClick={() => {
                  useWorkspace.getState().activate(sessionId, view.key);
                }}
                onDoubleClick={() => {
                  if (view.kind === "editor") setRenaming(view.key);
                }}
              >
                {view.kind === "table" && <span className="mr-1 text-zinc-500">ⓘ</span>}
                {view.kind === "data" && <span className="mr-1 text-zinc-500">▦</span>}
                {label(view)}
                {view.kind === "editor" && tabs[view.key]?.running && (
                  <span aria-label="running" className="ml-1 text-sky-400">
                    ●
                  </span>
                )}
              </button>
            )}
            <button
              type="button"
              aria-label={`Close ${label(view)}`}
              className="pr-2 text-xs text-zinc-500 hover:text-zinc-100"
              onClick={() => {
                onClose(view);
              }}
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <button
        type="button"
        aria-label="New tab"
        title={`New tab (${SHORTCUT_LABELS["new-tab"]})`}
        className="px-3 text-zinc-400 hover:text-zinc-100"
        onClick={() => {
          useEditor.getState().openTab(sessionId);
        }}
      >
        +
      </button>
    </div>
  );
}
