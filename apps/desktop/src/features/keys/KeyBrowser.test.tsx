import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useToasts } from "../../components/toastStore";
import { createMockBackend } from "../../test/mockBackend";
import { useWorkspace, workspaceOf } from "../workspace/workspaceStore";
import { KeyBrowser } from "./KeyBrowser";
import { useKeys } from "./keysStore";

let calls: { cmd: string; args: Record<string, unknown> }[] = [];

beforeEach(() => {
  calls = [];
  useKeys.setState({ lists: {} });
  useToasts.setState({ toasts: [] });
  const backend = createMockBackend();
  mockIPC((cmd, payload) => {
    calls.push({ cmd, args: (payload as { args?: Record<string, unknown> }).args ?? {} });
    return backend(cmd, payload);
  });
});

afterEach(() => {
  clearMocks();
});

const session = { connectionId: "c1", sessionId: "s1", label: "Cache" };
const keys = () => screen.getByRole("list", { name: "Keys" });

describe("KeyBrowser", () => {
  it("shows nothing without key-value sessions", () => {
    const { container } = render(<KeyBrowser sessions={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("scans, loads more, searches and opens a key", async () => {
    render(<KeyBrowser sessions={[session]} />);

    // user:1 and user:2 share a collapsed folder.
    const user = await within(keys()).findByRole("button", { name: "user, 2 keys" });
    expect(user).toHaveAttribute("aria-expanded", "false");
    expect(within(keys()).getAllByRole("button")).toHaveLength(1);
    await userEvent.click(user);
    expect(user).toHaveAttribute("aria-expanded", "true");
    expect(within(keys()).getByRole("button", { name: "user:1" })).toHaveTextContent("1");
    expect(within(keys()).getAllByRole("button")).toHaveLength(3);
    await userEvent.click(user);
    expect(within(keys()).getAllByRole("button")).toHaveLength(1);

    await userEvent.click(screen.getByRole("button", { name: /Load more \(2 so far\)/ }));
    expect(
      await within(keys()).findByRole("button", { name: "session, 1 key" }),
    ).toBeInTheDocument();

    await userEvent.clear(screen.getByLabelText("Key pattern"));
    await userEvent.type(screen.getByLabelText("Key pattern"), "nothing:*");
    await userEvent.click(screen.getByRole("button", { name: "Scan" }));
    expect(await screen.findByText("No keys match nothing:*.")).toBeInTheDocument();

    await userEvent.clear(screen.getByLabelText("Key pattern"));
    await userEvent.type(screen.getByLabelText("Key pattern"), "session:*");
    await userEvent.click(screen.getByRole("button", { name: "Scan" }));
    await userEvent.click(await within(keys()).findByRole("button", { name: "session, 1 key" }));
    await userEvent.click(within(keys()).getByRole("button", { name: "session:abc" }));
    expect(workspaceOf(useWorkspace.getState(), "s1").active).toBe(`key:${btoa("session:abc")}`);
  });

  it("creates a key after checking the name and previewing the command", async () => {
    render(<KeyBrowser sessions={[session]} />);
    await within(keys()).findAllByRole("button");

    await userEvent.click(screen.getByRole("button", { name: "New key" }));
    const dialog = screen.getByRole("dialog", { name: "New key" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Review" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("The key needs a name.");

    await userEvent.type(within(dialog).getByLabelText("Key name"), "user:1");
    await userEvent.click(within(dialog).getByRole("button", { name: "Review" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(/already exists/);

    await userEvent.clear(within(dialog).getByLabelText("Key name"));
    await userEvent.type(within(dialog).getByLabelText("Key name"), "user:9");
    await userEvent.selectOptions(within(dialog).getByLabelText("Type"), "hash");
    await userEvent.type(within(dialog).getByLabelText("field"), "name");
    await userEvent.type(within(dialog).getByLabelText("value"), "Grace");
    await userEvent.click(within(dialog).getByRole("button", { name: "Review" }));

    const preview = await screen.findByRole("dialog", { name: "Run these commands?" });
    expect(await within(preview).findByLabelText("Commands")).toHaveTextContent(
      "HSET user:9 name Grace",
    );
    // Cancel goes back to the form, which keeps what was typed.
    await userEvent.click(within(preview).getByRole("button", { name: "Cancel" }));
    expect(screen.getByLabelText("Key name")).toHaveValue("user:9");
    await userEvent.click(screen.getByRole("button", { name: "Review" }));
    const again = await screen.findByRole("dialog", { name: "Run these commands?" });
    await userEvent.click(await within(again).findByRole("button", { name: "Apply" }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(useToasts.getState().toasts.map((t) => t.message)).toContain("Created user:9");
    expect(workspaceOf(useWorkspace.getState(), "s1").active).toBe(`key:${btoa("user:9")}`);
    expect(calls.filter((c) => c.cmd === "kv_edit").map((c) => c.args.preview)).toEqual([
      true,
      true,
      false,
    ]);
  });

  it("refuses a bad sorted-set score before asking the backend", async () => {
    render(<KeyBrowser sessions={[session]} />);
    await within(keys()).findAllByRole("button");

    await userEvent.click(screen.getByRole("button", { name: "New key" }));
    const dialog = screen.getByRole("dialog", { name: "New key" });
    await userEvent.type(within(dialog).getByLabelText("Key name"), "board");
    await userEvent.selectOptions(within(dialog).getByLabelText("Type"), "zset");
    await userEvent.type(within(dialog).getByLabelText("member"), "ada");
    await userEvent.type(within(dialog).getByLabelText("score"), "high");
    await userEvent.click(within(dialog).getByRole("button", { name: "Review" }));

    expect(within(dialog).getByRole("alert")).toHaveTextContent("The score must be a number.");
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("offers no new key on a read-only connection", async () => {
    render(<KeyBrowser sessions={[{ ...session, readOnly: true }]} />);
    await within(keys()).findAllByRole("button");
    expect(screen.queryByRole("button", { name: "New key" })).toBeNull();
  });
});
