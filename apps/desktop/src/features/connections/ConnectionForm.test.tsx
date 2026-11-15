import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { ConnectionDraft, SecretKind } from "../../ipc";
import * as ipc from "../../ipc";
import {
  mysqlDriver,
  postgresDriver,
  postgresProfile,
  sqliteDriver,
  sqliteProfile,
} from "../../test/fixtures";
import { ConnectionForm } from "./ConnectionForm";
import { draftFromProfile, emptyDraft } from "./draft";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return {
    ...actual,
    connectionTest: vi.fn(),
    pickDatabaseFile: vi.fn(),
    pickNewDatabaseFile: vi.fn(),
    pickPemFile: vi.fn(),
    pickPrivateKeyFile: vi.fn(),
    sshTrustHost: vi.fn(),
    sshConfigHosts: vi.fn(),
  };
});

const drivers = [postgresDriver, mysqlDriver, sqliteDriver];

beforeEach(() => {
  vi.mocked(ipc.connectionTest).mockReset();
  vi.mocked(ipc.sshConfigHosts).mockResolvedValue([]);
});

function renderForm(
  initial: ConnectionDraft,
  { savedSecrets = [] as SecretKind[], onSave = vi.fn().mockResolvedValue(undefined) } = {},
) {
  render(
    <ConnectionForm
      initial={initial}
      drivers={drivers}
      savedSecrets={savedSecrets}
      onSave={onSave}
      onCancel={vi.fn()}
    />,
  );
  return onSave;
}

function savedDraft(onSave: ReturnType<typeof vi.fn>): ConnectionDraft | undefined {
  return onSave.mock.calls[0]?.[0] as ConnectionDraft | undefined;
}

describe("ConnectionForm with a file database", () => {
  it("needs a name and a file before saving", async () => {
    renderForm(emptyDraft(sqliteDriver));

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

    await userEvent.type(screen.getByLabelText("Name"), "Local");
    await userEvent.type(screen.getByLabelText("Database file"), "/tmp/a.db");

    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
    expect(screen.queryByLabelText("Host")).toBeNull();
    expect(screen.queryByText("TLS")).toBeNull();
  });

  it("fills the path from the pickers and sets the create option", async () => {
    vi.mocked(ipc.pickNewDatabaseFile).mockResolvedValue("/tmp/new.db");
    vi.mocked(ipc.pickDatabaseFile).mockResolvedValue("/tmp/old.db");
    const onSave = renderForm(draftFromProfile(sqliteProfile()));

    await userEvent.click(screen.getByRole("button", { name: "New file…" }));
    expect(screen.getByLabelText("Create the file if it does not exist")).toBeChecked();
    await userEvent.click(screen.getByRole("button", { name: "Browse…" }));
    expect(screen.getByLabelText("Create the file if it does not exist")).not.toBeChecked();
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(savedDraft(onSave)?.filePath).toBe("/tmp/old.db");
  });

  it("shows a save error", async () => {
    renderForm(draftFromProfile(sqliteProfile()), {
      onSave: vi.fn().mockRejectedValue({ code: "invalid_argument", message: "Bad name" }),
    });

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByText("Bad name")).toBeInTheDocument();
  });
});

describe("ConnectionForm with a network database", () => {
  it("switches to the chosen driver with its default port and TLS mode", async () => {
    renderForm(emptyDraft(sqliteDriver));

    await userEvent.selectOptions(screen.getByLabelText("Database type"), "postgres");

    expect(screen.getByLabelText("Host")).toHaveValue("localhost");
    expect(screen.getByLabelText("Port")).toHaveValue("5432");
    expect(screen.getByLabelText("Mode")).toHaveValue("prefer");
  });

  it("defaults to verify-full for a remote host until TLS is chosen by hand", async () => {
    renderForm(emptyDraft(postgresDriver));

    await userEvent.clear(screen.getByLabelText("Host"));
    await userEvent.type(screen.getByLabelText("Host"), "db.example.com");
    expect(screen.getByLabelText("Mode")).toHaveValue("verify-full");

    await userEvent.selectOptions(screen.getByLabelText("Mode"), "require");
    await userEvent.type(screen.getByLabelText("Host"), "x");
    expect(screen.getByLabelText("Mode")).toHaveValue("require");
  });

  it("warns about modes that do not verify the certificate", async () => {
    renderForm(draftFromProfile(postgresProfile()));

    expect(screen.queryByRole("note")).toBeNull();
    await userEvent.selectOptions(screen.getByLabelText("Mode"), "require");

    expect(screen.getByRole("note")).toHaveTextContent("not checked");
  });

  it("offers only the TLS modes the driver supports", () => {
    renderForm(emptyDraft(mysqlDriver));

    const modes = Array.from(screen.getByLabelText("Mode").querySelectorAll("option")).map(
      (o) => o.value,
    );

    expect(modes).toEqual(["disable", "prefer", "require", "verify-full"]);
  });

  it("picks CA and client files and requires certificate and key together", async () => {
    vi.mocked(ipc.pickPemFile).mockResolvedValue("/etc/client.pem");
    renderForm(draftFromProfile(postgresProfile()));

    await userEvent.click(screen.getAllByRole("button", { name: "Browse…" })[1] as HTMLElement);

    expect(screen.getByLabelText("Client certificate")).toHaveValue("/etc/client.pem");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("edits host, port, database and user", async () => {
    const onSave = renderForm(draftFromProfile(postgresProfile()));

    await userEvent.clear(screen.getByLabelText("Port"));
    await userEvent.type(screen.getByLabelText("Port"), "6543");
    await userEvent.clear(screen.getByLabelText("Database"));
    await userEvent.clear(screen.getByLabelText("User"));
    await userEvent.type(screen.getByLabelText("User"), "admin");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(savedDraft(onSave)).toMatchObject({ port: 6543, database: null, user: "admin" });
  });
});

describe("ConnectionForm passwords", () => {
  it("keeps a stored password when the field is left empty", async () => {
    const onSave = renderForm(draftFromProfile(postgresProfile()), {
      savedSecrets: ["db_password"] as SecretKind[],
    });

    expect(screen.getByLabelText("Password")).toHaveAttribute(
      "placeholder",
      "Stored in the keychain",
    );
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave.mock.calls[0]?.[1]).toBeUndefined();
  });

  it("sends a typed password", async () => {
    const onSave = renderForm(draftFromProfile(postgresProfile()), {
      savedSecrets: ["db_password"] as SecretKind[],
    });

    await userEvent.type(screen.getByLabelText("Password"), "s3cret");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave.mock.calls[0]?.[1]).toEqual({ dbPassword: "s3cret" });
  });

  it("can forget the stored password", async () => {
    const onSave = renderForm(draftFromProfile(postgresProfile()), {
      savedSecrets: ["db_password"] as SecretKind[],
    });

    await userEvent.click(screen.getByLabelText("Forget stored"));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(screen.getByLabelText("Password")).toHaveAttribute("placeholder", "Will be removed");
    expect(onSave.mock.calls[0]?.[1]).toEqual({ dbPassword: null });
  });

  it("tests with the typed password and reports the result", async () => {
    vi.mocked(ipc.connectionTest).mockResolvedValueOnce({
      serverVersion: "PostgreSQL 18.7",
      latencyMs: 3,
    });
    vi.mocked(ipc.connectionTest).mockRejectedValueOnce({
      code: "auth",
      message: "password authentication failed",
    });
    renderForm(draftFromProfile(postgresProfile()));

    await userEvent.type(screen.getByLabelText("Password"), "p");
    await userEvent.click(screen.getByRole("button", { name: "Test connection" }));
    expect(await screen.findByText("Connected: PostgreSQL 18.7 · 3 ms")).toBeInTheDocument();
    expect(vi.mocked(ipc.connectionTest).mock.calls[0]?.[1]).toEqual({ dbPassword: "p" });

    await userEvent.click(screen.getByRole("button", { name: "Test connection" }));
    expect(await screen.findByText("password authentication failed")).toBeInTheDocument();
  });
});

it("renders text options as inputs", async () => {
  const withText = {
    ...sqliteDriver,
    options: [
      { key: "mode", label: "Mode option", kind: "text" as const, default: "a", help: null },
    ],
  };
  const onSave = vi.fn().mockResolvedValue(undefined);
  render(
    <ConnectionForm
      initial={draftFromProfile(sqliteProfile())}
      drivers={[withText]}
      savedSecrets={[]}
      onSave={onSave}
      onCancel={vi.fn()}
    />,
  );

  await userEvent.clear(screen.getByLabelText("Mode option"));
  await userEvent.type(screen.getByLabelText("Mode option"), "b");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));

  expect(savedDraft(onSave)?.options.mode).toBe("b");
});

describe("ConnectionForm SSH tunnel", () => {
  it("is offered only for drivers that support tunnels", () => {
    renderForm(draftFromProfile(sqliteProfile()));

    expect(screen.queryByText("SSH tunnel")).toBeNull();
  });

  it("fills the tunnel from a ~/.ssh/config alias, with its jump host", async () => {
    vi.mocked(ipc.sshConfigHosts).mockResolvedValue([
      {
        alias: "gw",
        hostName: "gw.example.com",
        user: "ops",
        port: 2222,
        identityFile: null,
        proxyJump: null,
      },
      {
        alias: "db",
        hostName: "db.internal",
        user: "admin",
        port: null,
        identityFile: "/home/me/.ssh/id",
        proxyJump: "gw",
      },
    ]);
    const onSave = renderForm(draftFromProfile(postgresProfile()));

    await userEvent.click(screen.getByLabelText("Connect through an SSH server"));
    await userEvent.selectOptions(await screen.findByLabelText("Fill from ~/.ssh/config"), "db");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(savedDraft(onSave)?.ssh).toEqual({
      server: {
        host: "db.internal",
        port: 22,
        user: "admin",
        auth: "key_file",
        keyFile: "/home/me/.ssh/id",
      },
      jump: { host: "gw.example.com", port: 2222, user: "ops", auth: "agent", keyFile: null },
    });
  });

  it("saves a key-file tunnel behind a jump host with each hop's secret", async () => {
    vi.mocked(ipc.pickPrivateKeyFile).mockResolvedValue("/home/me/.ssh/id_ed25519");
    const onSave = renderForm(draftFromProfile(postgresProfile()));

    await userEvent.click(screen.getByLabelText("Connect through an SSH server"));
    await userEvent.type(screen.getByLabelText("SSH server host"), "bastion.internal");
    await userEvent.type(screen.getByLabelText("SSH server user"), "deploy");
    await userEvent.selectOptions(screen.getByLabelText("SSH server authentication"), "key_file");
    const server = screen.getByRole("group", { name: "SSH server" });
    await userEvent.click(within(server).getByRole("button", { name: "Browse…" }));
    await userEvent.type(screen.getByLabelText("SSH server key passphrase"), "phrase");
    await userEvent.click(screen.getByLabelText("Reach it through a jump host"));
    await userEvent.type(screen.getByLabelText("Jump host host"), "jump.example.com");
    await userEvent.type(screen.getByLabelText("Jump host user"), "me");
    await userEvent.selectOptions(screen.getByLabelText("Jump host authentication"), "password");
    await userEvent.type(screen.getByLabelText("Jump host password"), "jump-pw");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(savedDraft(onSave)?.ssh).toEqual({
      server: {
        host: "bastion.internal",
        port: 22,
        user: "deploy",
        auth: "key_file",
        keyFile: "/home/me/.ssh/id_ed25519",
      },
      jump: { host: "jump.example.com", port: 22, user: "me", auth: "password", keyFile: null },
    });
    expect(onSave.mock.calls[0]?.[1]).toEqual({
      sshKeyPassphrase: "phrase",
      jumpPassword: "jump-pw",
    });
  });

  it("shows stored SSH secrets and lets the tunnel be removed", async () => {
    const ssh = {
      server: { host: "b", port: 2222, user: "u", auth: "password" as const, keyFile: null },
      jump: null,
    };
    const onSave = renderForm(draftFromProfile(postgresProfile({ ssh })), {
      savedSecrets: ["ssh_password"] as SecretKind[],
    });

    expect(screen.getByLabelText("SSH server password")).toHaveAttribute(
      "placeholder",
      "Stored in the keychain",
    );
    expect(screen.getByLabelText("SSH server port")).toHaveValue("2222");
    await userEvent.click(screen.getByLabelText("Connect through an SSH server"));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(savedDraft(onSave)?.ssh).toBeNull();
  });

  it("asks to trust an unknown host key during a test and retries", async () => {
    const hostKey = { host: "b", port: 22, algorithm: "ssh-ed25519", fingerprint: "SHA256:k" };
    vi.mocked(ipc.connectionTest).mockRejectedValueOnce({
      code: "ssh_host_key_unknown",
      message: "not trusted",
      detail: { ...hostKey, stage: "host_key" },
    });
    vi.mocked(ipc.connectionTest).mockResolvedValueOnce({
      serverVersion: "PostgreSQL 18",
      latencyMs: 9,
    });
    vi.mocked(ipc.sshTrustHost).mockResolvedValue(undefined);
    renderForm(draftFromProfile(postgresProfile()));

    await userEvent.click(screen.getByRole("button", { name: "Test connection" }));
    await userEvent.click(await screen.findByRole("button", { name: "Trust and connect" }));

    expect(ipc.sshTrustHost).toHaveBeenCalledWith(hostKey);
    expect(await screen.findByText("Connected: PostgreSQL 18 · 9 ms")).toBeInTheDocument();
  });

  it("labels test failures with the failing stage", async () => {
    vi.mocked(ipc.connectionTest).mockRejectedValue({
      code: "ssh_auth",
      message: "rejected",
      detail: { stage: "ssh_auth" },
    });
    renderForm(draftFromProfile(postgresProfile()));

    await userEvent.click(screen.getByRole("button", { name: "Test connection" }));

    expect(await screen.findByText("SSH authentication failed: rejected")).toBeInTheDocument();
  });
});
