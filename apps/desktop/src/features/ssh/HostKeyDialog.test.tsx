import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { HostKeyDialog } from "./HostKeyDialog";

const hostKey = {
  host: "bastion",
  port: 2222,
  algorithm: "ssh-ed25519",
  fingerprint: "SHA256:abc",
};

describe("HostKeyDialog", () => {
  it("shows the fingerprint and trusts on request", async () => {
    const onTrust = vi.fn();
    render(
      <HostKeyDialog hostKey={hostKey} changed={false} onTrust={onTrust} onCancel={vi.fn()} />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Trust and connect" }));

    expect(screen.getByTestId("fingerprint")).toHaveTextContent("SHA256:abc");
    expect(screen.getByText(/bastion:2222 is not known yet/)).toBeInTheDocument();
    expect(onTrust).toHaveBeenCalled();
  });

  it("offers no way to proceed for a changed key", async () => {
    const onCancel = vi.fn();
    render(
      <HostKeyDialog
        hostKey={{ ...hostKey, port: 22 }}
        changed
        onTrust={vi.fn()}
        onCancel={onCancel}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Close" }));

    expect(screen.getByRole("alert")).toHaveTextContent("bastion is not the one trusted before");
    expect(screen.queryByRole("button", { name: "Trust and connect" })).toBeNull();
    expect(onCancel).toHaveBeenCalled();
  });
});
