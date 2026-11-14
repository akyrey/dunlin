import type { ConnectionProfile, DriverInfo } from "../../ipc";
import { sqliteDriver } from "../../test/fixtures";
import * as ipc from "../../ipc";
import { useConnections } from "./connectionsStore";
import { draftFromProfile, draftProblems, emptyDraft } from "./draft";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return {
    ...actual,
    driverList: vi.fn(),
    connectionList: vi.fn(),
    connectionSave: vi.fn(),
    connectionDelete: vi.fn(),
  };
});

function profile(id: string, name: string): ConnectionProfile {
  return { ...emptyDraft(sqliteDriver), id, name, filePath: "/tmp/a.db" };
}

beforeEach(() => {
  useConnections.setState({ drivers: [], profiles: [], loaded: false, error: null });
  vi.mocked(ipc.driverList).mockReset();
  vi.mocked(ipc.connectionList).mockReset();
});

describe("connections store", () => {
  it("loads drivers and profiles", async () => {
    vi.mocked(ipc.driverList).mockResolvedValue([sqliteDriver]);
    vi.mocked(ipc.connectionList).mockResolvedValue([profile("a", "A")]);

    await useConnections.getState().load();

    expect(useConnections.getState()).toMatchObject({
      drivers: [sqliteDriver],
      profiles: [profile("a", "A")],
      loaded: true,
      error: null,
    });
  });

  it("keeps the load error", async () => {
    vi.mocked(ipc.driverList).mockRejectedValue({ code: "internal", message: "boom" });
    vi.mocked(ipc.connectionList).mockResolvedValue([]);

    await useConnections.getState().load();

    expect(useConnections.getState().error).toEqual({ code: "internal", message: "boom" });
  });

  it("inserts or replaces saved profiles, sorted by name", async () => {
    useConnections.setState({ profiles: [profile("b", "beta")] });
    vi.mocked(ipc.connectionSave).mockResolvedValueOnce(profile("a", "Alpha"));
    vi.mocked(ipc.connectionSave).mockResolvedValueOnce(profile("b", "Zed"));

    await useConnections.getState().save(emptyDraft(sqliteDriver));
    await useConnections.getState().save(emptyDraft(sqliteDriver));

    expect(useConnections.getState().profiles.map((p) => p.name)).toEqual(["Alpha", "Zed"]);
  });

  it("removes deleted profiles", async () => {
    useConnections.setState({ profiles: [profile("a", "A"), profile("b", "B")] });
    vi.mocked(ipc.connectionDelete).mockResolvedValue(undefined);

    await useConnections.getState().remove("a");

    expect(useConnections.getState().profiles.map((p) => p.id)).toEqual(["b"]);
  });
});

describe("drafts", () => {
  it("start from the driver's option defaults", () => {
    const draft = emptyDraft(sqliteDriver);

    expect(draft.options).toEqual({ create_if_missing: "false" });
    expect(draft.id).toBeNull();
  });

  it("copy options from a profile instead of sharing them", () => {
    const source = profile("a", "A");

    const draft = draftFromProfile(source);
    draft.options.extra = "1";

    expect(source.options).toEqual({ create_if_missing: "false" });
  });

  it("report missing name and file", () => {
    expect(draftProblems(emptyDraft(sqliteDriver), sqliteDriver)).toEqual([
      "Enter a name",
      "Choose a database file",
    ]);
    expect(draftProblems(profile("a", "A"), sqliteDriver)).toEqual([]);
  });

  it("require a host for network drivers", () => {
    const tcp: DriverInfo = { ...sqliteDriver, endpoint: "tcp" };

    expect(draftProblems({ ...emptyDraft(tcp), name: "x" }, tcp)).toEqual(["Enter a host"]);
  });
});
