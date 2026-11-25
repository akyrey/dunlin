import { type KeyboardEvent, type MouseEvent, useCallback, useState } from "react";

import { ContextMenu, type ContextMenuItem } from "@roost/ui";

import type { ConnectionProfile } from "../../ipc";
import { ConnectionMark } from "./ConnectionMark";
import { groupByFolder } from "./folders";

type Props = {
  profiles: ConnectionProfile[];
  openIds: Set<string>;
  /** Open when the app last ran; they reconnect when opened. */
  rememberedIds?: ReadonlySet<string>;
  activeId: string | null;
  onOpen: (profile: ConnectionProfile) => void;
  onEdit: (profile: ConnectionProfile) => void;
  onDelete: (profile: ConnectionProfile) => void;
  onDuplicate?: (profile: ConnectionProfile) => void;
  onMove?: (profile: ConnectionProfile, step: -1 | 1) => void;
  /** Reloads an open connection's schema (right-click menu). */
  onRefresh?: (profile: ConnectionProfile) => void;
  /** Closes an open connection (right-click menu). */
  onDisconnect?: (profile: ConnectionProfile) => void;
};

type Menu = { profile: ConnectionProfile; x: number; y: number };

type RowProps = Props & {
  profile: ConnectionProfile;
  onMenu: (menu: Menu) => void;
};

const ACTION = "text-xs text-zinc-400 hover:text-zinc-100";

function Row(props: RowProps) {
  const { profile, openIds, rememberedIds = new Set(), activeId } = props;
  const { onOpen, onEdit, onDelete, onDuplicate, onMove, onMenu } = props;
  const isOpen = openIds.has(profile.id);
  const hasMenu = isOpen && (props.onRefresh !== undefined || props.onDisconnect !== undefined);
  const onContextMenu = (event: MouseEvent<HTMLLIElement>) => {
    if (!hasMenu) return;
    event.preventDefault();
    onMenu({ profile, x: event.clientX, y: event.clientY });
  };
  const remembered = rememberedIds.has(profile.id);
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const step = { ArrowUp: -1, ArrowDown: 1 }[event.key] as -1 | 1 | undefined;
    if (!event.altKey || step === undefined || !onMove) return;
    event.preventDefault();
    onMove(profile, step);
  };
  return (
    <li
      onContextMenu={onContextMenu}
      className={`group flex items-center gap-2 px-3 py-1.5 text-sm ${
        profile.id === activeId ? "bg-zinc-800" : "hover:bg-zinc-800/60"
      }`}
    >
      <ConnectionMark driver={profile.driver} color={profile.color} />
      <button
        type="button"
        className="flex-1 truncate text-left"
        onClick={() => {
          onOpen(profile);
        }}
        onKeyDown={onKeyDown}
        title={profile.filePath ?? profile.host ?? ""}
      >
        {profile.name}
        {isOpen && <span className="sr-only"> (open)</span>}
        {remembered && <span className="sr-only"> (open last time)</span>}
      </button>
      {remembered && (
        <span
          aria-hidden="true"
          title="Open last time: reconnects when you open it"
          className="h-1.5 w-1.5 rounded-full border border-emerald-400"
        />
      )}
      {isOpen && <span aria-hidden="true" className="h-1.5 w-1.5 rounded-full bg-emerald-400" />}
      {profile.readOnly && (
        <span className="rounded bg-amber-900/60 px-1 text-[10px] uppercase text-amber-300">
          ro
        </span>
      )}
      <span className="hidden gap-1 group-hover:flex group-focus-within:flex">
        {onMove && (
          <>
            <button
              type="button"
              className={ACTION}
              aria-label={`Move ${profile.name} up`}
              title="Move up (⌥↑)"
              onClick={() => {
                onMove(profile, -1);
              }}
            >
              ↑
            </button>
            <button
              type="button"
              className={ACTION}
              aria-label={`Move ${profile.name} down`}
              title="Move down (⌥↓)"
              onClick={() => {
                onMove(profile, 1);
              }}
            >
              ↓
            </button>
          </>
        )}
        {onDuplicate && (
          <button
            type="button"
            className={ACTION}
            aria-label={`Duplicate ${profile.name}`}
            onClick={() => {
              onDuplicate(profile);
            }}
          >
            Copy
          </button>
        )}
        <button
          type="button"
          className={ACTION}
          aria-label={`Edit ${profile.name}`}
          onClick={() => {
            onEdit(profile);
          }}
        >
          Edit
        </button>
        <button
          type="button"
          className="text-xs text-zinc-400 hover:text-rose-300"
          aria-label={`Delete ${profile.name}`}
          onClick={() => {
            onDelete(profile);
          }}
        >
          Delete
        </button>
      </span>
    </li>
  );
}

/** Saved connections, grouped by folder (task 8.13). Folders collapse; the order is the user's. */
export function ConnectionList(props: Props) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [menu, setMenu] = useState<Menu | null>(null);
  const closeMenu = useCallback(() => {
    setMenu(null);
  }, []);
  if (props.profiles.length === 0) {
    return (
      <p className="px-3 py-2 text-xs text-zinc-500">
        No connections yet. Add one with New connection.
      </p>
    );
  }
  const toggle = (folder: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (!next.delete(folder)) next.add(folder);
      return next;
    });
  };
  return (
    <>
      <ul aria-label="Connections" className="flex flex-col">
        {groupByFolder(props.profiles).map(({ folder, profiles }) => {
          const rows = profiles.map((profile) => (
            <Row key={profile.id} {...props} profile={profile} onMenu={setMenu} />
          ));
          if (folder === null) return rows;
          const open = !collapsed.has(folder);
          return (
            <li key={`folder:${folder}`}>
              <button
                type="button"
                aria-expanded={open}
                className="flex w-full items-center gap-1 px-3 py-1 text-left text-xs font-semibold uppercase tracking-wide text-zinc-500 hover:text-zinc-300"
                onClick={() => {
                  toggle(folder);
                }}
              >
                <span aria-hidden="true">{open ? "▾" : "▸"}</span>
                {folder}
              </button>
              {open && (
                <ul aria-label={folder} className="flex flex-col">
                  {rows}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
      {menu && (
        <ContextMenu
          label={`Actions for ${menu.profile.name}`}
          x={menu.x}
          y={menu.y}
          items={connectionMenuItems(menu.profile, props)}
          onClose={closeMenu}
        />
      )}
    </>
  );
}

function connectionMenuItems(profile: ConnectionProfile, props: Props): ContextMenuItem[] {
  const { onRefresh, onDisconnect } = props;
  return [
    ...(onRefresh
      ? [
          {
            label: "Refresh schema",
            onSelect: () => {
              onRefresh(profile);
            },
          },
        ]
      : []),
    ...(onDisconnect
      ? [
          {
            label: "Close connection",
            destructive: true,
            onSelect: () => {
              onDisconnect(profile);
            },
          },
        ]
      : []),
  ];
}
