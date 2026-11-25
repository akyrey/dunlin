import { sqliteProfile, sessionFor } from "../../test/fixtures";
import { type PaletteHandlers, paletteCommands, toPaletteItem } from "./commands";

const handlers = (): PaletteHandlers => ({
  newConnection: vi.fn(),
  openSettings: vi.fn(),
  openShortcuts: vi.fn(),
  checkForUpdates: vi.fn(),
  newTab: vi.fn(),
  refreshSchema: vi.fn(),
  queryStats: vi.fn(),
  disconnect: vi.fn(),
  openConnection: vi.fn(),
  openTable: vi.fn(),
  openSavedQuery: vi.fn(),
});

const main = { catalog: null, schema: "main" };

describe("paletteCommands", () => {
  it("offers query statistics only where the server records them", () => {
    const profile = sqliteProfile();
    const base = sessionFor(profile);
    const withStats = { ...base, capabilities: { ...base.capabilities, queryStats: true } };
    const on = handlers();
    const labels = (session: typeof base) =>
      paletteCommands(
        {
          profiles: [profile],
          sessions: { [profile.id]: session },
          activeConnectionId: profile.id,
          tables: [],
          savedQueries: [],
        },
        on,
      );
    expect(labels(base).some((c) => c.label === "Query statistics")).toBe(false);
    labels(withStats)
      .find((c) => c.label === "Query statistics")
      ?.run();
    expect(on.queryStats).toHaveBeenCalledWith(base.sessionId);
  });

  it("offers only app actions and connections without an open session", () => {
    const profile = sqliteProfile();
    const commands = paletteCommands(
      { profiles: [profile], sessions: {}, activeConnectionId: null, tables: [], savedQueries: [] },
      handlers(),
    );

    expect(commands.map((c) => c.label)).toEqual([
      "New connection",
      "Settings",
      "Keyboard shortcuts",
      "Check for updates",
      profile.name,
    ]);
  });

  it("offers the active session's actions, tables and saved queries", () => {
    const profile = sqliteProfile();
    const session = sessionFor(profile);
    const on = handlers();
    const commands = paletteCommands(
      {
        profiles: [profile],
        sessions: { [profile.id]: session },
        activeConnectionId: profile.id,
        tables: [{ namespace: main, name: "people" }],
        savedQueries: [
          { id: "q1", connectionId: null, name: "Everyone", sql: "SELECT 1", updatedAt: 0 },
        ],
      },
      on,
    );
    const byLabel = (label: string) => commands.find((c) => c.label === label);

    byLabel("people")?.run();
    byLabel("New editor tab")?.run();
    byLabel("Disconnect")?.run();

    expect(byLabel("people")).toMatchObject({ group: "Tables", detail: "main" });
    expect(byLabel("Everyone")).toMatchObject({ detail: "every connection" });
    expect(byLabel(profile.name)).toMatchObject({ detail: "open" });
    expect(on.openTable).toHaveBeenCalledWith(session.sessionId, {
      namespace: main,
      name: "people",
    });
    expect(on.newTab).toHaveBeenCalledWith(session.sessionId);
    expect(on.disconnect).toHaveBeenCalledWith(profile.id);
  });
});

describe("toPaletteItem", () => {
  it("shows a command's shortcut the platform's way", () => {
    const run = vi.fn();
    expect(
      toPaletteItem(
        { id: "s", group: "Actions", label: "Settings", shortcut: "settings", run },
        true,
      ),
    ).toEqual({
      id: "s",
      group: "Actions",
      label: "Settings",
      hint: "⌘,",
      run,
    });
    expect(
      toPaletteItem({ id: "c", group: "Connections", label: "db", detail: "open", run }),
    ).toEqual({
      id: "c",
      group: "Connections",
      label: "db",
      detail: "open",
      run,
    });
  });
});
