import type { SessionInfo } from "../../ipc";
import * as ipc from "../../ipc";
import { sessionFor, sqliteProfile } from "../../test/fixtures";
import { useSessions } from "./sessionsStore";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return { ...actual, sessionOpen: vi.fn(), sessionClose: vi.fn() };
});

const session: SessionInfo = sessionFor(sqliteProfile());

beforeEach(() => {
  useSessions.setState({ sessions: {}, activeConnectionId: null });
  vi.mocked(ipc.sessionOpen).mockReset();
  vi.mocked(ipc.sessionClose).mockReset();
});

describe("sessions store", () => {
  it("opens a session once and activates it", async () => {
    vi.mocked(ipc.sessionOpen).mockResolvedValue(session);

    await useSessions.getState().open("c1");
    await useSessions.getState().open("c1");

    expect(ipc.sessionOpen).toHaveBeenCalledTimes(1);
    expect(useSessions.getState()).toMatchObject({
      sessions: { c1: session },
      activeConnectionId: "c1",
    });
  });

  it("closes a session and clears the active connection", async () => {
    useSessions.setState({ sessions: { c1: session }, activeConnectionId: "c1" });
    vi.mocked(ipc.sessionClose).mockResolvedValue(undefined);

    await useSessions.getState().close("c1");

    expect(ipc.sessionClose).toHaveBeenCalledWith("s1");
    expect(useSessions.getState()).toMatchObject({ sessions: {}, activeConnectionId: null });
  });

  it("ignores closing an unknown connection", async () => {
    await useSessions.getState().close("missing");

    expect(ipc.sessionClose).not.toHaveBeenCalled();
  });

  it("keeps another active connection when forgetting one", () => {
    useSessions.setState({ sessions: { c1: session }, activeConnectionId: "c2" });

    useSessions.getState().forget("c1");

    expect(useSessions.getState()).toMatchObject({ sessions: {}, activeConnectionId: "c2" });
  });

  it("activates a connection", () => {
    useSessions.getState().activate("c9");

    expect(useSessions.getState().activeConnectionId).toBe("c9");
  });
});
