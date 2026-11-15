import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { KeyEditDialog } from "./KeyEditDialog";

afterEach(() => {
  clearMocks();
});

function backend(preview: () => unknown, run: () => unknown) {
  mockIPC((_cmd, payload) => {
    const args = (payload as { args: { preview: boolean } }).args;
    return args.preview ? preview() : run();
  });
}

const show = (removes = false) => {
  const onApplied = vi.fn();
  const onClose = vi.fn();
  render(
    <KeyEditDialog
      sessionId="s1"
      keyId={btoa("k")}
      edit={{ kind: "delete" }}
      removes={removes}
      onApplied={onApplied}
      onClose={onClose}
    />,
  );
  return { onApplied, onClose };
};

describe("KeyEditDialog", () => {
  it("shows the planned commands and runs them on Apply", async () => {
    backend(
      () => ["DEL k"],
      () => ["DEL k"],
    );
    const { onApplied } = show(true);

    expect(screen.getByText("Preparing…")).toBeInTheDocument();
    expect(await screen.findByLabelText("Commands")).toHaveTextContent("DEL k");
    await userEvent.click(screen.getByRole("button", { name: "Apply" }));

    await waitFor(() => {
      expect(onApplied).toHaveBeenCalled();
    });
  });

  it("shows why a preview failed and keeps Apply off", async () => {
    backend(
      () => Promise.reject({ code: "invalid_argument", message: "The score must be finite" }),
      () => [],
    );
    show();

    expect(await screen.findByRole("alert")).toHaveTextContent("The score must be finite");
    expect(screen.getByRole("button", { name: "Apply" })).toBeDisabled();
  });

  it("shows why the run failed and lets it be tried again", async () => {
    backend(
      () => ["DEL k"],
      () => Promise.reject({ code: "read_only", message: "The connection is read-only" }),
    );
    const { onApplied, onClose } = show();

    await screen.findByLabelText("Commands");
    await userEvent.click(screen.getByRole("button", { name: "Apply" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("The connection is read-only");
    expect(screen.getByRole("button", { name: "Apply" })).toBeEnabled();
    expect(onApplied).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });
});
