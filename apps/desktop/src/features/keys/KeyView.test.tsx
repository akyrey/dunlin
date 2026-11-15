import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen } from "@testing-library/react";

import { createMockBackend } from "../../test/mockBackend";
import { KeyView } from "./KeyView";

beforeEach(() => {
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
});
