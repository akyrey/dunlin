import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import * as ipc from "../../ipc";
import { useEdits } from "./editStore";
import { QuitGuard } from "./quitGuard";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return { ...actual, onQuitRequested: vi.fn(), quitApp: vi.fn(() => Promise.resolve()) };
});

let ask: () => boolean = () => false;
const stop = vi.fn();

beforeEach(() => {
  useEdits.setState({ pending: {}, previewSkipped: {} });
  vi.mocked(ipc.onQuitRequested).mockImplementation((handler) => {
    ask = handler;
    return Promise.resolve(stop);
  });
  vi.mocked(ipc.quitApp).mockClear();
});

describe("QuitGuard", () => {
  it("lets the window close without pending edits", async () => {
    render(<QuitGuard />);
    await act(() => Promise.resolve());

    expect(ask()).toBe(false);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("keeps the window open with pending edits and quits on confirmation", async () => {
    const { unmount } = render(<QuitGuard />);
    await act(() => Promise.resolve());
    useEdits.getState().setPending("view:a", "s1", 2);

    let kept = false;
    act(() => {
      kept = ask();
    });

    expect(kept).toBe(true);
    expect(screen.getByRole("dialog", { name: "Quit with unapplied changes?" })).toHaveTextContent(
      "2 changes have",
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    act(() => {
      ask();
    });
    await userEvent.click(screen.getByRole("button", { name: "Quit and discard" }));
    expect(ipc.quitApp).toHaveBeenCalled();
    unmount();
    expect(stop).toHaveBeenCalled();
  });
});
