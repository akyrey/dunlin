import * as ipc from "../../ipc";
import { sessionFor, sqliteProfile } from "../../test/fixtures";
import { useEdits } from "../edit/editStore";
import { disconnect } from "./disconnect";
import { useSessions } from "./sessionsStore";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return { ...actual, sessionClose: vi.fn() };
});

const session = sessionFor(sqliteProfile());

beforeEach(() => {
  useSessions.setState({ sessions: { c1: session }, activeConnectionId: "c1" });
  useEdits.setState({ pending: {} });
  vi.mocked(ipc.sessionClose).mockReset();
  vi.mocked(ipc.sessionClose).mockResolvedValue(undefined);
});

describe("disconnect", () => {
  it("closes the connection's session", async () => {
    const problem = await disconnect("c1");

    expect(problem).toBeNull();
    expect(ipc.sessionClose).toHaveBeenCalledWith(session.sessionId);
    expect(useSessions.getState().sessions).toEqual({});
  });

  it("refuses while a table in the session has unapplied changes", async () => {
    useEdits.setState({ pending: { v1: { sessionId: session.sessionId, count: 2 } } });

    const problem = await disconnect("c1");

    expect(problem).toMatch(/unapplied/);
    expect(ipc.sessionClose).not.toHaveBeenCalled();
  });

  it("reports a backend failure instead of throwing", async () => {
    vi.mocked(ipc.sessionClose).mockRejectedValue({ code: "internal", message: "boom" });

    expect(await disconnect("c1")).toBe("boom");
  });
});
