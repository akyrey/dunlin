import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useConnections } from "../connections/connectionsStore";
import { useSessions } from "../sessions/sessionsStore";
import { postgresProfile, sessionFor, sqliteProfile } from "../../test/fixtures";
import { ConnectionSelect } from "./ConnectionSelect";
import { useEditor } from "./editorStore";

const local = sqliteProfile({ id: "local", name: "Local" });
const prod = postgresProfile({ id: "prod", name: "Prod" });
const now = (action: () => void) => {
  action();
};

beforeEach(() => {
  useConnections.setState({ profiles: [local, prod] });
  useSessions.setState({ sessions: {}, activeConnectionId: "local" });
});

describe("ConnectionSelect", () => {
  it("opens the chosen connection and moves the tab to it", async () => {
    const open = vi.spyOn(useSessions.getState(), "open").mockResolvedValue(sessionFor(prod, "s2"));
    const moveTab = vi.spyOn(useEditor.getState(), "moveTab").mockResolvedValue();
    render(<ConnectionSelect tabId="t1" connectionId="local" disabled={false} guard={now} />);

    await userEvent.selectOptions(screen.getByLabelText("Connection"), "prod");

    await waitFor(() => {
      expect(moveTab).toHaveBeenCalledWith("t1", "s2");
    });
    expect(open).toHaveBeenCalledWith("prod");
  });

  it("shows why the connection could not be opened, and keeps the tab", async () => {
    vi.spyOn(useSessions.getState(), "open").mockRejectedValue({
      code: "connection_failed",
      message: "refused",
    });
    const moveTab = vi.spyOn(useEditor.getState(), "moveTab").mockResolvedValue();
    render(<ConnectionSelect tabId="t1" connectionId="local" disabled={false} guard={now} />);

    await userEvent.selectOptions(screen.getByLabelText("Connection"), "prod");

    expect(await screen.findByRole("alert")).toHaveTextContent("refused");
    expect(moveTab).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Connection")).toHaveValue("local");
  });

  it("cannot be changed while disabled", () => {
    render(<ConnectionSelect tabId="t1" connectionId="local" disabled guard={now} />);

    expect(screen.getByLabelText("Connection")).toBeDisabled();
  });
});
