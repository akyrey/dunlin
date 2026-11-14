import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { sqliteProfile } from "../../test/fixtures";
import { ConnectionList } from "./ConnectionList";

describe("ConnectionList", () => {
  it("says when there are no connections", () => {
    render(
      <ConnectionList
        profiles={[]}
        openIds={new Set()}
        activeId={null}
        onOpen={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByText(/^No connections yet\./)).toBeInTheDocument();
  });

  it("opens, edits and deletes", async () => {
    const profile = sqliteProfile({ readOnly: true });
    const onOpen = vi.fn();
    const onEdit = vi.fn();
    const onDelete = vi.fn();
    render(
      <ConnectionList
        profiles={[profile]}
        openIds={new Set([profile.id])}
        activeId={profile.id}
        onOpen={onOpen}
        onEdit={onEdit}
        onDelete={onDelete}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /^Local/ }));
    await userEvent.click(screen.getByRole("button", { name: "Edit Local" }));
    await userEvent.click(screen.getByRole("button", { name: "Delete Local" }));

    expect(onOpen).toHaveBeenCalledWith(profile);
    expect(onEdit).toHaveBeenCalledWith(profile);
    expect(onDelete).toHaveBeenCalledWith(profile);
    expect(screen.getByText("ro")).toBeInTheDocument();
  });
});
