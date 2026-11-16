import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "./App";
import { sshTrustHost } from "./ipc";
import { useConnections } from "./features/connections/connectionsStore";
import { useEditor } from "./features/editor/editorStore";
import { useSessions } from "./features/sessions/sessionsStore";
import { DEFAULT_SETTINGS } from "./features/settings/defaults";
import { useSettings } from "./features/settings/settingsStore";
import { useUpdates } from "./features/updates/updateStore";
import {
  MOCK_DATABASE_PATH,
  MOCK_DROP_TUNNEL,
  MOCK_FINGERPRINT,
  createMockBackend,
} from "./test/mockBackend";

beforeEach(() => {
  useConnections.setState({ drivers: [], profiles: [], loaded: false, error: null });
  useSessions.setState({ sessions: {}, activeConnectionId: null });
  useEditor.setState({ tabs: {} });
  useSettings.setState({ settings: DEFAULT_SETTINGS, loaded: false, error: null });
  useUpdates.setState({ status: { kind: "idle" } });
  mockIPC(createMockBackend());
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

async function addConnection(name: string, readOnly = false) {
  await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
  const dialog = screen.getByRole("dialog");
  await userEvent.selectOptions(within(dialog).getByLabelText("Database type"), "sqlite");
  await userEvent.type(within(dialog).getByLabelText("Name"), name);
  await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
  if (readOnly) await userEvent.click(within(dialog).getByLabelText("Read-only"));
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => {
    expect(screen.queryByRole("dialog")).toBeNull();
  });
}

async function openAndRun(name: string, sql: string) {
  await userEvent.click(screen.getByRole("button", { name: new RegExp(`^${name}`) }));
  await screen.findByRole("button", { name: "Run" });
  const sessionId = Object.keys(useEditor.getState().tabs)[0] ?? "";
  useEditor.getState().setText(sessionId, sql);
  await userEvent.click(screen.getByRole("button", { name: "Run" }));
}

/** The mock backend, with `settings_get` answering `checkForUpdates` and calls recorded. */
function backendWithUpdates(checkForUpdates: boolean, called: string[]) {
  const backend = createMockBackend();
  mockIPC((cmd, payload) => {
    called.push(cmd);
    if (cmd === "settings_get") return { ...DEFAULT_SETTINGS, checkForUpdates };
    if (cmd === "update_check") {
      return { version: "9.0.0", currentVersion: "0.1.1", notes: null };
    }
    return backend(cmd, payload);
  });
}

describe("App", () => {
  it("checks for updates at startup and offers the update", async () => {
    backendWithUpdates(true, []);
    render(<App />);

    expect(await screen.findByRole("dialog", { name: "Update available" })).toBeTruthy();
  });

  it("does not check for updates at startup when the setting is off", async () => {
    const called: string[] = [];
    backendWithUpdates(false, called);
    render(<App />);

    await waitFor(() => {
      expect(useSettings.getState().loaded).toBe(true);
    });
    expect(called).toContain("settings_get");
    expect(called).not.toContain("update_check");
  });

  it("adds a connection, opens it, runs a query and shows rows", async () => {
    render(<App />);

    await addConnection("People");
    expect(screen.getByRole("list", { name: "Connections" })).toHaveTextContent("People");

    await openAndRun("People", "SELECT * FROM people");

    expect(await screen.findByText("Ada")).toBeInTheDocument();
    expect(screen.getByText("9007199254740993")).toBeInTheDocument();
    expect(screen.getByText("NULL")).toBeInTheDocument();
    expect(screen.getByText("3 rows · 3 ms")).toBeInTheDocument();
    expect(useConnections.getState().profiles[0]?.filePath).toBe(MOCK_DATABASE_PATH);
  });

  it("shows the server error for a syntax error", async () => {
    render(<App />);
    await addConnection("People");

    await openAndRun("People", "SELECT * FRM people");

    expect(await screen.findByRole("alert")).toHaveTextContent('near "FRM": syntax error');
    expect(screen.getByRole("button", { name: "line 1, column 10" })).toBeInTheDocument();
  });

  it("asks before a dangerous statement and runs it on confirmation", async () => {
    render(<App />);
    await addConnection("People");

    await openAndRun("People", "DELETE FROM people");
    const dialog = await screen.findByRole("dialog", { name: "Run a dangerous statement?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Run anyway" }));

    expect(await screen.findByText(/^2 rows affected/)).toBeInTheDocument();
  });

  it("rejects writes on a read-only connection", async () => {
    render(<App />);
    await addConnection("Prod", true);

    await openAndRun("Prod", "INSERT INTO people VALUES (4)");

    expect(await screen.findByRole("alert")).toHaveTextContent("read-only");
    expect(screen.getByText("read-only", { selector: "span" })).toBeInTheDocument();
  });

  it("deletes a connection after confirmation", async () => {
    render(<App />);
    await addConnection("People");

    await userEvent.click(screen.getByRole("button", { name: "Delete People" }));
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Delete" }),
    );

    await waitFor(() => {
      expect(screen.getByText(/^No connections yet\./)).toBeInTheDocument();
    });
  });

  it("edits a connection", async () => {
    render(<App />);
    await addConnection("People");

    await userEvent.click(screen.getByRole("button", { name: "Edit People" }));
    const name = within(screen.getByRole("dialog")).getByLabelText("Name");
    await userEvent.clear(name);
    await userEvent.type(name, "Renamed");
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Save" }));

    expect(await screen.findByRole("button", { name: /^Renamed/ })).toBeInTheDocument();
  });

  it("shows an error when a session cannot be opened", async () => {
    const backend = createMockBackend();
    mockIPC((cmd, payload) => {
      if (cmd === "session_open")
        return Promise.reject({ code: "connect", message: "unable to open" });
      return backend(cmd, payload);
    });
    useConnections.setState({ profiles: [], drivers: [] });
    render(<App />);
    await addConnection("Broken");

    await userEvent.click(screen.getByRole("button", { name: /^Broken/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent("unable to open");
  });

  it("cancels a running statement and shows the elapsed time meanwhile", async () => {
    render(<App />);
    await addConnection("People");

    await openAndRun("People", "SELECT pg_sleep(30)");
    expect(await screen.findByRole("timer")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("canceling statement");
    expect(screen.queryByRole("timer")).toBeNull();
    expect(screen.getByRole("button", { name: "Run" })).toBeEnabled();
  });

  it("saves a network connection with its password and shows it as stored", async () => {
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
    const dialog = screen.getByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "Staging");
    await userEvent.type(within(dialog).getByLabelText("Password"), "s3cret");
    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });

    await userEvent.click(screen.getByRole("button", { name: "Edit Staging" }));

    expect(within(screen.getByRole("dialog")).getByLabelText("Password")).toHaveAttribute(
      "placeholder",
      "Stored in the keychain",
    );
    expect(useConnections.getState().profiles[0]?.savedSecrets).toEqual(["db_password"]);
  });

  async function addTunnelledConnection(name: string, sshHost: string) {
    await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
    const dialog = screen.getByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), name);
    await userEvent.type(within(dialog).getByLabelText("Password"), "db-pw");
    await userEvent.click(within(dialog).getByLabelText("Connect through an SSH server"));
    await userEvent.type(within(dialog).getByLabelText("SSH server host"), sshHost);
    await userEvent.type(within(dialog).getByLabelText("SSH server user"), "deploy");
    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  }

  it("asks to trust an unknown SSH host once, then connects", async () => {
    render(<App />);
    await addTunnelledConnection("Behind bastion", "bastion.example.com");

    await userEvent.click(screen.getByRole("button", { name: /^Behind bastion/ }));
    const prompt = await screen.findByRole("dialog", { name: "Trust this SSH server?" });
    expect(within(prompt).getByTestId("fingerprint")).toHaveTextContent(MOCK_FINGERPRINT);
    await userEvent.click(within(prompt).getByRole("button", { name: "Trust and connect" }));

    expect(await screen.findByRole("button", { name: "Run" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await userEvent.click(await screen.findByRole("button", { name: "Trusted SSH hosts…" }));
    const trusted = await screen.findByRole("list", { name: "Trusted hosts" });
    expect(trusted).toHaveTextContent("bastion.example.com:22");
    await userEvent.click(
      within(trusted).getByRole("button", { name: "Remove bastion.example.com:22" }),
    );
    expect(await screen.findByText("No hosts trusted yet.")).toBeInTheDocument();
  });

  it("declining an unknown key connects to nothing and stores nothing", async () => {
    render(<App />);
    await addTunnelledConnection("Behind bastion", "bastion.example.com");

    await userEvent.click(screen.getByRole("button", { name: /^Behind bastion/ }));
    const prompt = await screen.findByRole("dialog", { name: "Trust this SSH server?" });
    await userEvent.click(within(prompt).getByRole("button", { name: "Cancel" }));
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    await userEvent.click(await screen.findByRole("button", { name: "Trusted SSH hosts…" }));

    expect(await screen.findByText("No hosts trusted yet.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Run" })).toBeNull();
  });

  it("refuses a changed SSH host key with no way to proceed", async () => {
    render(<App />);
    await addTunnelledConnection("Moved", "changed.example.com");
    // Trusted earlier with the real key; the host now presents a different one.
    await sshTrustHost({
      host: "changed.example.com",
      port: 22,
      algorithm: "ssh-ed25519",
      fingerprint: MOCK_FINGERPRINT,
    });

    await userEvent.click(screen.getByRole("button", { name: /^Moved/ }));
    const refusal = await screen.findByRole("dialog", { name: "SSH host key has changed" });

    expect(within(refusal).getByRole("alert")).toHaveTextContent("will not connect");
    expect(within(refusal).queryByRole("button", { name: "Trust and connect" })).toBeNull();
  });

  it("closes the session and explains when the SSH tunnel is lost", async () => {
    render(<App />);
    await addTunnelledConnection("Behind bastion", "bastion.example.com");
    await userEvent.click(screen.getByRole("button", { name: /^Behind bastion/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Trust and connect" }));
    await screen.findByRole("button", { name: "Run" });
    const sessionId = Object.keys(useEditor.getState().tabs)[0] ?? "";
    useEditor.getState().setText(sessionId, MOCK_DROP_TUNNEL);

    await userEvent.click(screen.getByRole("button", { name: "Run" }));

    expect(
      await screen.findByText(/The SSH connection was lost\. Open the connection again/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Run" })).toBeNull();
    expect(screen.getByText(/^Open a connection to start writing SQL/)).toBeInTheDocument();
  });
});
