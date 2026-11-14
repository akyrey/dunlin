import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import * as ipc from "../../ipc";
import { sqliteDriver, sqliteProfile } from "../../test/fixtures";
import { ConnectionForm } from "./ConnectionForm";
import { draftFromProfile, emptyDraft } from "./draft";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return {
    ...actual,
    connectionTest: vi.fn(),
    pickDatabaseFile: vi.fn(),
    pickNewDatabaseFile: vi.fn(),
  };
});

beforeEach(() => {
  vi.mocked(ipc.connectionTest).mockReset();
});

function renderForm(onSave = vi.fn().mockResolvedValue(undefined)) {
  render(
    <ConnectionForm
      initial={emptyDraft(sqliteDriver)}
      driver={sqliteDriver}
      onSave={onSave}
      onCancel={vi.fn()}
    />,
  );
  return onSave;
}

describe("ConnectionForm", () => {
  it("needs a name and a file before saving", async () => {
    renderForm();

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Enter a name · Choose a database file");

    await userEvent.type(screen.getByLabelText("Name"), "Local");
    await userEvent.type(screen.getByLabelText("Database file"), "/tmp/a.db");

    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
  });

  it("saves the edited draft", async () => {
    const onSave = renderForm();
    await userEvent.type(screen.getByLabelText("Name"), "Local");
    await userEvent.type(screen.getByLabelText("Database file"), "/tmp/a.db");
    await userEvent.click(screen.getByLabelText("Read-only"));

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ name: "Local", filePath: "/tmp/a.db", readOnly: true }),
    );
  });

  it("shows a save error", async () => {
    renderForm(vi.fn().mockRejectedValue({ code: "invalid_argument", message: "Bad name" }));
    await userEvent.type(screen.getByLabelText("Name"), "Local");
    await userEvent.type(screen.getByLabelText("Database file"), "/tmp/a.db");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByText("Bad name")).toBeInTheDocument();
  });

  it("tests the connection and reports success or failure", async () => {
    vi.mocked(ipc.connectionTest).mockResolvedValueOnce({
      serverVersion: "SQLite 3.50",
      latencyMs: 2,
    });
    vi.mocked(ipc.connectionTest).mockRejectedValueOnce({
      code: "connect",
      message: "unable to open",
    });
    render(
      <ConnectionForm
        initial={draftFromProfile(sqliteProfile())}
        driver={sqliteDriver}
        onSave={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Test connection" }));
    expect(await screen.findByText("Connected: SQLite 3.50 · 2 ms")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Test connection" }));
    expect(await screen.findByText("unable to open")).toBeInTheDocument();
  });

  it("fills the path from the file pickers and sets the create option", async () => {
    vi.mocked(ipc.pickNewDatabaseFile).mockResolvedValue("/tmp/new.db");
    vi.mocked(ipc.pickDatabaseFile).mockResolvedValue("/tmp/old.db");
    const onSave = renderForm();
    await userEvent.type(screen.getByLabelText("Name"), "X");

    await userEvent.click(screen.getByRole("button", { name: "New file…" }));
    expect(screen.getByLabelText("Database file")).toHaveValue("/tmp/new.db");
    expect(screen.getByLabelText("Create the file if it does not exist")).toBeChecked();

    await userEvent.click(screen.getByRole("button", { name: "Browse…" }));
    expect(screen.getByLabelText("Create the file if it does not exist")).not.toBeChecked();

    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ filePath: "/tmp/old.db" }));
  });

  it("keeps the path when a picker is cancelled", async () => {
    vi.mocked(ipc.pickDatabaseFile).mockResolvedValue(null);
    render(
      <ConnectionForm
        initial={draftFromProfile(sqliteProfile())}
        driver={sqliteDriver}
        onSave={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Browse…" }));

    expect(screen.getByLabelText("Database file")).toHaveValue("/tmp/local.db");
  });

  it("renders text options as inputs", async () => {
    const driver = {
      ...sqliteDriver,
      options: [{ key: "mode", label: "Mode", kind: "text" as const, default: "a", help: null }],
    };
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <ConnectionForm
        initial={draftFromProfile(sqliteProfile())}
        driver={driver}
        onSave={onSave}
        onCancel={vi.fn()}
      />,
    );

    await userEvent.clear(screen.getByLabelText("Mode"));
    await userEvent.type(screen.getByLabelText("Mode"), "b");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    const saved = onSave.mock.calls[0]?.[0] as ipc.ConnectionDraft | undefined;
    expect(saved?.options.mode).toBe("b");
  });
});
