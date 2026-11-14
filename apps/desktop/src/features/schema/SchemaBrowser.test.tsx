import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import { copyText } from "../../lib/clipboard";
import { createMockBackend } from "../../test/mockBackend";
import { useConnections } from "../connections/connectionsStore";
import { useEditor } from "../editor/editorStore";
import { useSessions } from "../sessions/sessionsStore";
import { useWorkspace } from "../workspace/workspaceStore";
import { useSchema } from "./schemaStore";

vi.mock("../../lib/clipboard", () => ({ copyText: vi.fn(() => Promise.resolve()) }));

type Backend = ReturnType<typeof createMockBackend>;
let calls: string[] = [];

function install(override?: (cmd: string, args: unknown) => unknown) {
  const backend: Backend = createMockBackend();
  mockIPC((cmd, payload) => {
    calls.push(cmd);
    const overridden = override?.(cmd, (payload as { args?: unknown } | undefined)?.args);
    return overridden === undefined ? backend(cmd, payload) : overridden;
  });
}

beforeEach(() => {
  calls = [];
  useConnections.setState({ drivers: [], profiles: [], loaded: false, error: null });
  useSessions.setState({ sessions: {}, activeConnectionId: null });
  useEditor.setState({ tabs: {} });
  useSchema.setState({ trees: {}, filter: "", showSystem: false });
  useWorkspace.setState({ workspaces: {} });
  vi.mocked(copyText).mockClear();
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

async function openConnection() {
  await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
  const dialog = screen.getByRole("dialog");
  await userEvent.selectOptions(within(dialog).getByLabelText("Database type"), "sqlite");
  await userEvent.type(within(dialog).getByLabelText("Name"), "People");
  await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => {
    expect(screen.queryByRole("dialog")).toBeNull();
  });
  await userEvent.click(screen.getByRole("button", { name: /^People/ }));
  return screen.findByRole("tree", { name: "Schema tree" });
}

describe("schema browser", () => {
  it("opens to the only namespace and lists tables and views", async () => {
    install();
    render(<App />);

    const tree = await openConnection();

    expect(await within(tree).findByRole("button", { name: "people" })).toBeInTheDocument();
    expect(within(tree).getByRole("button", { name: /Tables/ })).toBeInTheDocument();
    expect(within(tree).getByRole("button", { name: /Views/ })).toBeInTheDocument();
    expect(within(tree).getByRole("button", { name: "rich_people" })).toBeInTheDocument();
    expect(within(tree).queryByRole("button", { name: /temp/ })).toBeNull();
  });

  it("filters, shows system namespaces and collapses groups", async () => {
    install();
    render(<App />);
    const tree = await openConnection();
    await within(tree).findByRole("button", { name: "people" });

    await userEvent.type(screen.getByLabelText("Filter tables"), "ord");
    expect(within(tree).queryByRole("button", { name: "people" })).toBeNull();
    expect(within(tree).getByRole("button", { name: "orders" })).toBeInTheDocument();
    await userEvent.clear(screen.getByLabelText("Filter tables"));
    await userEvent.click(screen.getByLabelText("Show system objects"));
    expect(within(tree).getByRole("button", { name: /temp/ })).toBeInTheDocument();
    await userEvent.click(within(tree).getByRole("button", { name: /Tables/ }));
    expect(within(tree).queryByRole("button", { name: "people" })).toBeNull();
  });

  it("shows a node-level error and keeps the rest of the tree", async () => {
    install((cmd) =>
      cmd === "schema_objects"
        ? Promise.reject({ code: "permission", message: "permission denied for schema main" })
        : undefined,
    );
    render(<App />);

    const tree = await openConnection();

    expect(await within(tree).findByRole("alert")).toHaveTextContent("permission denied");
    expect(within(tree).getByRole("button", { name: /main/ })).toBeInTheDocument();
  });

  it("opens table details with columns, indexes, constraints and DDL, and follows a foreign key", async () => {
    install();
    render(<App />);
    const tree = await openConnection();

    await userEvent.click(await within(tree).findByRole("button", { name: "orders" }));
    expect(await screen.findByRole("heading", { name: "main.orders" })).toBeInTheDocument();
    expect(await screen.findByRole("cell", { name: "person_id" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Constraints" }));
    const foreignKeys = screen.getByRole("region", { name: "Foreign keys" });
    expect(foreignKeys).toHaveTextContent("CASCADE");
    await userEvent.click(within(foreignKeys).getByRole("button", { name: /main\.people/ }));

    expect(await screen.findByRole("heading", { name: "main.people" })).toBeInTheDocument();
    expect(screen.getByText(/rows addressed by id/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Indexes" }));
    expect(screen.getByRole("cell", { name: "people_name_idx" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "DDL" }));
    expect(await screen.findByLabelText("Table DDL")).toHaveTextContent("CREATE TABLE people");
    expect(screen.getAllByRole("tab", { name: /orders|people/ })).toHaveLength(2);

    await userEvent.click(screen.getByRole("button", { name: "Close people" }));
    expect(await screen.findByRole("heading", { name: "main.orders" })).toBeInTheDocument();
  });

  it("copies names and generates a SELECT into the editor from the context menu", async () => {
    install();
    render(<App />);
    const tree = await openConnection();
    const actions = async () => {
      await userEvent.click(
        await within(tree).findByRole("button", { name: "Actions for people" }),
      );
      return screen.getByRole("menu", { name: "Actions for people" });
    };

    await userEvent.click(within(await actions()).getByRole("menuitem", { name: "Copy name" }));
    await userEvent.click(
      within(await actions()).getByRole("menuitem", { name: "Copy qualified name" }),
    );
    await userEvent.click(
      within(await actions()).getByRole("menuitem", { name: "Generate SELECT" }),
    );

    expect(vi.mocked(copyText).mock.calls).toEqual([["people"], ['"main"."people"']]);
    const sessionId = Object.keys(useEditor.getState().tabs)[0] ?? "";
    expect(useEditor.getState().tabs[sessionId]?.text).toBe(
      'SELECT "id", "name", "balance"\nFROM "main"."people";',
    );
    expect(screen.getByRole("button", { name: "Run" })).toBeInTheDocument();
  });

  it("closes the menu on Escape, and opens it with a right click", async () => {
    install();
    render(<App />);
    const tree = await openConnection();
    const row = (await within(tree).findByRole("button", { name: "people" })).parentElement;
    if (!row) throw new Error("no row");

    await userEvent.pointer({ keys: "[MouseRight]", target: row });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    await userEvent.click(within(tree).getByRole("button", { name: "people" }));
    expect(await screen.findByRole("heading", { name: "main.people" })).toBeInTheDocument();
  });

  it("reports a failed action and refreshes through the backend", async () => {
    install((cmd) =>
      cmd === "schema_snippets"
        ? Promise.reject({ code: "not_found", message: "gone" })
        : undefined,
    );
    render(<App />);
    const tree = await openConnection();

    await userEvent.click(await within(tree).findByRole("button", { name: "Actions for people" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Generate SELECT" }));
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));

    expect(await screen.findByText("gone")).toBeInTheDocument();
    await waitFor(() => {
      expect(calls).toContain("schema_refresh");
    });
  });
});
