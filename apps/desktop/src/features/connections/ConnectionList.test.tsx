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

describe("ConnectionList context menu", () => {
  const renderList = (openIds: Set<string>) => {
    const profile = sqliteProfile();
    const onRefresh = vi.fn();
    const onDisconnect = vi.fn();
    render(
      <ConnectionList
        profiles={[profile]}
        openIds={openIds}
        activeId={null}
        onOpen={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onRefresh={onRefresh}
        onDisconnect={onDisconnect}
      />,
    );
    return { profile, onRefresh, onDisconnect };
  };

  it("offers refresh and close on an open connection", async () => {
    const profile = sqliteProfile();
    const onRefresh = vi.fn();
    const onDisconnect = vi.fn();
    render(
      <ConnectionList
        profiles={[profile]}
        openIds={new Set([profile.id])}
        activeId={profile.id}
        onOpen={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onRefresh={onRefresh}
        onDisconnect={onDisconnect}
      />,
    );

    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByRole("button", { name: /^Local/ }),
    });
    const menu = screen.getByRole("menu", { name: "Actions for Local" });
    expect(menu).toBeInTheDocument();
    await userEvent.click(screen.getByRole("menuitem", { name: "Refresh schema" }));
    expect(onRefresh).toHaveBeenCalledWith(profile);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByRole("button", { name: /^Local/ }),
    });
    await userEvent.click(screen.getByRole("menuitem", { name: "Close connection" }));
    expect(onDisconnect).toHaveBeenCalledWith(profile);
  });

  it("closes the menu on Escape", async () => {
    const profile = sqliteProfile();
    render(
      <ConnectionList
        profiles={[profile]}
        openIds={new Set([profile.id])}
        activeId={null}
        onOpen={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onRefresh={vi.fn()}
        onDisconnect={vi.fn()}
      />,
    );

    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByRole("button", { name: /^Local/ }),
    });
    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("shows no menu on a connection that is not open", async () => {
    const { profile } = renderList(new Set());

    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByRole("button", { name: profile.name }),
    });

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
