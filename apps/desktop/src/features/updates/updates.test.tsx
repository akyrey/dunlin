import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { useToasts } from "../../components/toastStore";
import type { UpdateInfo } from "../../ipc";
import { useEdits } from "../edit/editStore";
import { UpdatePrompt } from "./UpdatePrompt";
import { useUpdates } from "./updateStore";

const update: UpdateInfo = { version: "1.2.0", currentVersion: "1.1.0", notes: "Faster grids." };

type Backend = { check: () => unknown; install?: (channel: unknown) => unknown };

/** Routes the two update commands; `install` gets the progress channel's id. */
function backend({ check, install }: Backend) {
  mockIPC((cmd, payload) => {
    if (cmd === "update_check") return check();
    if (cmd === "update_install") {
      return install?.((payload as { onProgress: unknown }).onProgress) ?? new Promise(() => {});
    }
    return null;
  });
}

const reject = (message: string) => () => Promise.reject(new Error(message));

const toastMessages = () => useToasts.getState().toasts.map((t) => t.message);

beforeEach(() => {
  useUpdates.setState({ status: { kind: "idle" } });
  useToasts.setState({ toasts: [] });
  useEdits.setState({ pending: {}, previewSkipped: {} });
});

afterEach(() => {
  clearMocks();
});

describe("update checks", () => {
  it("offers a found update with its notes", async () => {
    backend({ check: () => update });
    render(<UpdatePrompt />);

    await act(() => useUpdates.getState().check("startup"));

    expect(screen.getByRole("dialog", { name: "Update available" })).toBeTruthy();
    expect(screen.getByText("Dunlin 1.2.0 is available. You have 1.1.0.")).toBeTruthy();
    expect(screen.getByText("Faster grids.")).toBeTruthy();
  });

  it("says nothing at startup when up to date or when the check fails", async () => {
    backend({ check: () => null });
    await useUpdates.getState().check("startup");
    backend({ check: reject("offline") });
    await useUpdates.getState().check("startup");

    expect(useUpdates.getState().status).toEqual({ kind: "idle" });
    expect(toastMessages()).toEqual([]);
  });

  it("reports every outcome of a manual check", async () => {
    backend({ check: () => null });
    await useUpdates.getState().check("manual");
    backend({ check: reject("Could not check for updates: offline") });
    await useUpdates.getState().check("manual");

    expect(toastMessages()).toEqual([
      "Dunlin is up to date",
      "Could not check for updates: offline",
    ]);
  });

  it("Later hides the prompt", async () => {
    backend({ check: () => update });
    render(<UpdatePrompt />);
    await act(() => useUpdates.getState().check("manual"));

    await userEvent.click(screen.getByRole("button", { name: "Later" }));

    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("warns that unapplied changes are lost by the restart", async () => {
    useEdits.getState().setPending("view:a", "s1", 2);
    backend({ check: () => update });
    render(<UpdatePrompt />);

    await act(() => useUpdates.getState().check("manual"));

    expect(screen.getByText(/2 changes have not been applied and will be lost/)).toBeTruthy();
  });

  it("shows the download and keeps the prompt while installing", async () => {
    backend({ check: () => update });
    render(<UpdatePrompt />);
    await act(() => useUpdates.getState().check("manual"));

    await userEvent.click(screen.getByRole("button", { name: "Install and restart" }));
    act(() => {
      useUpdates.setState({
        status: { kind: "installing", update, downloaded: 512, total: 1024 },
      });
    });

    expect(screen.getByRole("status").textContent).toBe("Downloading… 50%");
    expect(screen.getByRole("button", { name: "Later" })).toHaveProperty("disabled", true);
  });

  it("shows a failed install and lets the user try again", async () => {
    let attempts = 0;
    backend({
      check: () => update,
      install: () => {
        attempts += 1;
        return Promise.reject({ code: "update_failed", message: "Could not install the update" });
      },
    });
    render(<UpdatePrompt />);
    await act(() => useUpdates.getState().check("manual"));

    await userEvent.click(screen.getByRole("button", { name: "Install and restart" }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toBe("Could not install the update");
    });
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));

    await waitFor(() => {
      expect(attempts).toBe(2);
    });
  });
});
