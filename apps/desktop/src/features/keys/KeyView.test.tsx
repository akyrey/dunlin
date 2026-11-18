import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useToasts } from "@roost/ui";
import { useWorkspace, workspaceOf } from "../workspace/workspaceStore";

import { createMockBackend } from "../../test/mockBackend";
import { KeyView } from "./KeyView";

beforeEach(() => {
  useToasts.setState({ toasts: [] });
  mockIPC(createMockBackend());
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

describe("KeyView", () => {
  it("fills the string editor with the current value", async () => {
    render(<KeyView sessionId="s1" id={btoa("session:abc")} name="session:abc" />);
    expect(await screen.findByLabelText("Value")).toHaveValue("token");
    expect(screen.getByRole("button", { name: "Delete key" })).toBeInTheDocument();
  });

  it("offers no editing on a read-only connection", async () => {
    render(<KeyView sessionId="s1" id={btoa("session:abc")} name="session:abc" readOnly />);
    expect(await screen.findByText("TTL 90 s")).toBeInTheDocument();
    expect(screen.queryByRole("form", { name: "Edit value" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Delete key" })).toBeNull();
  });

  const toasts = () => useToasts.getState().toasts.map((t) => t.message);
  const preview = () => screen.findByRole("dialog", { name: "Run these commands?" });

  it("fills the editor from the selected row and saves a field after the preview", async () => {
    render(<KeyView sessionId="s1" id={btoa("user:1")} name="user:1" />);

    await userEvent.click(await screen.findByRole("gridcell", { name: "Ada" }));
    const editor = screen.getByRole("form", { name: "Edit value" });
    expect(within(editor).getByLabelText("Field")).toHaveValue("name");
    await userEvent.clear(within(editor).getByLabelText("Value"));
    await userEvent.type(within(editor).getByLabelText("Value"), "Grace");
    await userEvent.click(within(editor).getByRole("button", { name: "Set field" }));
    const dialog = await preview();
    await userEvent.click(await within(dialog).findByRole("button", { name: "Apply" }));

    expect(await screen.findByRole("gridcell", { name: "Grace" })).toBeInTheDocument();
    expect(toasts()).toContain("Saved");
  });

  it("checks a TTL before asking, and persists when it is empty", async () => {
    render(<KeyView sessionId="s1" id={btoa("session:abc")} name="session:abc" />);
    await screen.findByText("TTL 90 s");

    await userEvent.type(screen.getByLabelText("TTL in seconds"), "soon");
    await userEvent.click(screen.getByRole("button", { name: "Set TTL" }));
    expect(toasts()).toContain("The TTL must be a positive number of seconds.");

    await userEvent.clear(screen.getByLabelText("TTL in seconds"));
    await userEvent.click(screen.getByRole("button", { name: "Persist" }));
    const dialog = await preview();
    expect(await within(dialog).findByLabelText("Commands")).toHaveTextContent("EXPIRE");
    await userEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
    await waitFor(() => {
      expect(toasts()).toContain("TTL updated");
    });
  });

  it("renames into a new tab and deletes, closing the key's tab", async () => {
    const workspace = useWorkspace.getState();
    workspace.openKey("s1", btoa("session:abc"), "session:abc");
    render(<KeyView sessionId="s1" id={btoa("session:abc")} name="session:abc" />);
    await screen.findByText("TTL 90 s");

    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    expect(toasts()).toContain("The new name is the current name.");
    await userEvent.clear(screen.getByLabelText("New key name"));
    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    expect(toasts()).toContain("The new name cannot be empty.");

    await userEvent.type(screen.getByLabelText("New key name"), "session:xyz");
    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    let dialog = await preview();
    await userEvent.click(await within(dialog).findByRole("button", { name: "Apply" }));
    await waitFor(() => {
      expect(workspaceOf(useWorkspace.getState(), "s1").active).toBe(`key:${btoa("session:xyz")}`);
    });
    expect(toasts()).toContain("Key renamed");

    await userEvent.click(screen.getByRole("button", { name: "Delete key" }));
    dialog = await preview();
    expect(within(dialog).getByRole("button", { name: "Apply" })).toHaveClass("bg-rose-700");
    await userEvent.click(await within(dialog).findByRole("button", { name: "Apply" }));
    await waitFor(() => {
      expect(toasts()).toContain("Deleted session:abc");
    });
  });

  it("says when a key no longer exists", async () => {
    render(<KeyView sessionId="s1" id={btoa("gone")} name="gone" />);
    expect(await screen.findByRole("alert")).toHaveTextContent("The key no longer exists");
  });
});
