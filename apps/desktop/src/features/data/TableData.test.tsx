import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { createMockBackend } from "../../test/mockBackend";
import { TableData } from "./TableData";

const main = { catalog: null, schema: "main" };
let calls: { cmd: string; args: Record<string, unknown> }[] = [];
/** Statements the backend refused because the view's connection was busy. */
let refused = 0;

beforeEach(() => {
  calls = [];
  refused = 0;
  const backend = createMockBackend();
  mockIPC((cmd, payload) => {
    calls.push({
      cmd,
      args: (payload as { args?: Record<string, unknown> } | undefined)?.args ?? {},
    });
    const result = backend(cmd, payload);
    if (result instanceof Promise) {
      result.catch((error: unknown) => {
        if ((error as { message?: string }).message?.includes("already running")) refused++;
      });
    }
    return result;
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

function show(
  name: string,
  match: Parameters<typeof TableData>[0]["match"] = [],
  readOnly = false,
) {
  const onFollow = vi.fn();
  const view = render(
    <TableData
      sessionId="s1"
      connectionId="c1"
      table={{ namespace: main, name }}
      match={match}
      onFollow={onFollow}
      viewKey={`view:${name}`}
      readOnly={readOnly}
      profile={{ name: "People", color: "#22aa88", alwaysPreview: false }}
    />,
  );
  return { onFollow, ...view };
}

const status = () => screen.getByRole("status", { name: "Rows shown" });

describe("TableData", () => {
  it("pages forward and back, changes the page size and counts exactly", async () => {
    show("events");

    expect(await screen.findByText(/Rows 1–200 of ~450 rows/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => {
      expect(status()).toHaveTextContent("Rows 201–400");
    });
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => {
      expect(status()).toHaveTextContent("Rows 401–450");
    });
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Previous page" }));
    await waitFor(() => {
      expect(status()).toHaveTextContent("Rows 201–400");
    });

    await userEvent.selectOptions(screen.getByLabelText("Page size"), "500");
    await waitFor(() => {
      expect(status()).toHaveTextContent("Rows 1–450");
    });
    await userEvent.click(screen.getByRole("button", { name: "Count exactly" }));
    await waitFor(() => {
      expect(status()).toHaveTextContent("of 450 rows");
    });
    expect(screen.queryByRole("button", { name: "Count exactly" })).toBeNull();
  });

  it("seeks from each page's cursor, back as well as forward, and restarts on a sort", async () => {
    show("events");
    await screen.findByText(/Rows 1–200/);
    const afters = () =>
      calls.filter((c) => c.cmd === "table_page").map((c) => JSON.stringify(c.args.after ?? null));

    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => {
      expect(status()).toHaveTextContent("Rows 201–400");
    });
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => {
      expect(status()).toHaveTextContent("Rows 401–450");
    });
    await userEvent.click(screen.getByRole("button", { name: "Previous page" }));
    await waitFor(() => {
      expect(status()).toHaveTextContent("Rows 201–400");
    });
    const cursor = (n: number) => JSON.stringify([{ t: "int", v: String(n) }]);
    expect(afters()).toEqual(["null", cursor(200), cursor(400), cursor(200)]);

    fireEvent.click(screen.getByRole("button", { name: "Sort by id" }));
    await waitFor(() => {
      expect(calls.filter((c) => c.cmd === "table_page").at(-1)?.args.sort).toEqual([
        { column: "id", descending: false },
      ]);
    });
    expect(afters().at(-1)).toBe("null");
  });

  it("imports a CSV file with its columns mapped by header name, then reloads", async () => {
    show("people");
    await screen.findByText(/Rows 1–/);

    await userEvent.click(screen.getByRole("button", { name: "Import CSV…" }));
    const dialog = await screen.findByRole("dialog", { name: "Import CSV into people" });
    const source = (column: string) => within(dialog).getByLabelText(`Source for ${column}`);
    await waitFor(() => {
      expect(source("id")).toHaveValue("1");
    });
    expect(source("name")).toHaveValue("0");
    expect(source("balance")).toHaveValue("2");
    expect(within(dialog).getByRole("table", { name: "Column mapping" })).toHaveTextContent(
      "Zed, Yan",
    );

    await userEvent.selectOptions(source("balance"), "");
    await userEvent.click(within(dialog).getByRole("button", { name: "Import" }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    const imported = calls.find((c) => c.cmd === "import_csv");
    expect(imported?.args.columns).toEqual([
      { column: "id", source: 1 },
      { column: "name", source: 0 },
    ]);
    expect(await screen.findByRole("gridcell", { name: "Zed" })).toBeInTheDocument();
  });

  it("offers no import on a read-only connection", async () => {
    show("people", [], true);
    await screen.findByText(/Rows 1–/);
    expect(screen.queryByRole("button", { name: "Import CSV…" })).toBeNull();
  });

  it("keeps the last page under a filter error, remembers filters, and clears", async () => {
    show("events");
    await screen.findByText(/Rows 1–200/);
    const filter = screen.getByLabelText("Filter");

    await userEvent.type(filter, "kind = 'login'{Enter}");
    await waitFor(() => {
      expect(status()).toHaveTextContent("Rows 1–150");
    });
    await userEvent.clear(filter);
    await userEvent.type(filter, "kind FRM x{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("syntax error");
    expect(status()).toHaveTextContent("Rows 1–150");
    expect(document.querySelectorAll("datalist option")).toHaveLength(2);

    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    await waitFor(() => {
      expect(status()).toHaveTextContent("Rows 1–200");
    });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("sorts on the server, refreshes, and offers cancel while loading", async () => {
    show("events");
    await screen.findByText(/Rows 1–200/);

    fireEvent.click(screen.getByRole("button", { name: "Sort by id" }));
    fireEvent.click(screen.getByRole("button", { name: "Sort by id" }));
    await waitFor(() => {
      expect(screen.getByRole("columnheader", { name: "id" })).toHaveAttribute(
        "aria-sort",
        "descending",
      );
    });
    await waitFor(() => {
      expect(calls.filter((c) => c.cmd === "table_page").at(-1)?.args.sort).toEqual([
        { column: "id", descending: true },
      ]);
    });

    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await userEvent.type(screen.getByLabelText("Filter"), "pg_sleep(30) > 0{Enter}");
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("canceling statement");
    expect(calls.some((c) => c.cmd === "query_cancel")).toBe(true);
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });

  it("supersedes a loading page instead of running two at once", async () => {
    show("events");
    await screen.findByText(/Rows 1–200/);

    for (let i = 0; i < 4; i++) fireEvent.click(screen.getByRole("button", { name: "Sort by id" }));

    await waitFor(() => {
      expect(screen.getByRole("columnheader", { name: "id" })).toHaveAttribute(
        "aria-sort",
        "ascending",
      );
    });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(refused).toBe(0);
  });

  it("warns about typographic quotes and straightens them", async () => {
    show("events");
    await userEvent.type(screen.getByLabelText("Filter"), "kind = \u201Clogin\u201D");

    await userEvent.click(screen.getByRole("button", { name: "Use straight quotes" }));

    expect(screen.getByLabelText("Filter")).toHaveValue('kind = "login"');
    expect(screen.getByLabelText("Filter")).toHaveAttribute("spellcheck", "false");
    expect(screen.queryByRole("button", { name: "Use straight quotes" })).toBeNull();
  });

  it("shows a match as a removable chip and follows foreign keys", async () => {
    const { onFollow } = show("people", [{ column: "id", value: { t: "int", v: "2" } }]);
    expect(await screen.findByText("id = 2")).toBeInTheDocument();
    expect(await screen.findByRole("gridcell", { name: "Grace" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Show every row" }));
    expect(await screen.findByRole("gridcell", { name: "Ada" })).toBeInTheDocument();
    expect(onFollow).not.toHaveBeenCalled();
  });

  it("follows a foreign key to the referenced row", async () => {
    const { onFollow } = show("orders");

    const links = await screen.findAllByRole("button", { name: "Open the main.people row" });
    await userEvent.click(links[0] as HTMLElement);

    expect(onFollow).toHaveBeenCalledWith({ namespace: main, name: "people" }, [
      { column: "id", value: { t: "int", v: "1" } },
    ]);
  });

  it("loads a truncated cell in full in the viewer and closes its connections", async () => {
    const { unmount } = show("documents");
    const cell = await screen.findByRole("gridcell", { name: /Quarterly/ });

    await userEvent.dblClick(cell);
    const viewer = await screen.findByRole("complementary", { name: "Cell viewer" });
    await userEvent.click(within(viewer).getByRole("button", { name: "Load the whole value" }));
    await waitFor(() => {
      expect(within(viewer).getByLabelText("Cell value")).toHaveTextContent('"items": [');
    });
    unmount();

    await waitFor(() => {
      expect(calls.filter((c) => c.cmd === "tab_close")).toHaveLength(2);
    });
  });
});
