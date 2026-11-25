import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { SchemaEditArgs, SchemaEditPreview, TableInfo } from "../../ipc";
import { useEdits } from "../edit/editStore";
import { TableDetails } from "./TableDetails";
import { useSchemaEdits } from "./schemaEditStore";

const info: TableInfo = {
  table: { namespace: { catalog: null, schema: "main" }, name: "orders" },
  kind: "table",
  comment: null,
  columns: [
    {
      name: "id",
      typeName: "INTEGER",
      category: "integer",
      nullable: false,
      default: null,
      comment: null,
      ordinal: 1,
      generated: false,
    },
    {
      name: "total",
      typeName: "INTEGER",
      category: "integer",
      nullable: true,
      default: null,
      comment: null,
      ordinal: 2,
      generated: false,
    },
  ],
  primaryKey: { name: null, columns: ["id"] },
  indexes: [
    { name: "orders_total_idx", columns: ["total"], unique: false, method: null, predicate: null },
  ],
  foreignKeys: [],
  uniqueConstraints: [],
  checkConstraints: [],
  rowKey: ["id"],
  rowId: null,
};

type Handler = (cmd: string, args: unknown) => unknown;

let edits: SchemaEditArgs[] = [];
let applied: SchemaEditArgs[] = [];

function install(overrides: Record<string, Handler> = {}) {
  mockIPC((cmd, payload) => {
    const args = (payload as { args?: unknown } | undefined)?.args;
    const override = overrides[cmd];
    if (override) return override(cmd, args);
    switch (cmd) {
      case "schema_table":
        return info;
      case "schema_table_ddl":
        return "CREATE TABLE orders (id INTEGER PRIMARY KEY, total INTEGER);";
      case "schema_edit_preview": {
        edits.push(args as SchemaEditArgs);
        return {
          statements: ['ALTER TABLE "main"."orders" ADD COLUMN "shipped" INTEGER'],
          warnings: [],
          destructive: false,
          transactional: true,
          statementOffset: 0,
        } satisfies SchemaEditPreview;
      }
      case "schema_edit_apply":
        applied.push(args as SchemaEditArgs);
        return { statements: 1 };
      default:
        return null;
    }
  });
}

function renderDetails(editable = true) {
  return render(
    <TableDetails
      sessionId="s1"
      table={info.table}
      onOpenTable={() => undefined}
      viewKey="table-1"
      editable={editable}
      connection={{ name: "Shop", color: "#f00" }}
    />,
  );
}

beforeEach(() => {
  edits = [];
  applied = [];
  useSchemaEdits.setState({ pending: {}, sessions: {} });
  useEdits.setState({ pending: {}, previewSkipped: {} });
});

afterEach(() => {
  clearMocks();
});

async function startEditing() {
  await userEvent.click(await screen.findByRole("button", { name: "Edit" }));
}

describe("schema editing", () => {
  it("is not offered on a read-only session", async () => {
    install();
    renderDetails(false);
    expect(await screen.findAllByText("INTEGER", { selector: "code" })).toHaveLength(2);
    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
  });

  it("adds a column, previews and applies it", async () => {
    install();
    renderDetails();
    await startEditing();

    await userEvent.click(screen.getByRole("button", { name: "Add column" }));
    const row = screen.getByRole("row", { name: /New column/ });
    await userEvent.type(within(row).getByLabelText("Name"), "shipped");
    await userEvent.type(within(row).getByLabelText("Type"), "INTEGER");

    expect(screen.getByText("1 change not applied")).toBeDefined();
    expect(useEdits.getState().pending["table-1"]?.count).toBe(1);

    await userEvent.click(screen.getByRole("button", { name: "Preview and apply" }));
    const dialog = await screen.findByRole("dialog", { name: "Apply schema changes?" });
    expect(within(dialog).getByText(/in one transaction/)).toBeDefined();
    expect(edits[0]?.edit.changes).toEqual([
      {
        kind: "add_column",
        column: {
          name: "shipped",
          typeName: "INTEGER",
          nullable: true,
          default: null,
          comment: null,
        },
      },
    ]);

    await userEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
    await waitFor(() => {
      expect(applied).toHaveLength(1);
    });
    expect(applied[0]?.confirmed).toBe(false);
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(useEdits.getState().pending["table-1"]).toBeUndefined();
    expect(screen.queryByRole("toolbar", { name: "Schema changes" })).toBeNull();
  });

  it("confirms a destructive plan by applying it as such", async () => {
    install({
      schema_edit_preview: () => ({
        statements: ['DROP INDEX "main"."orders_total_idx"'],
        warnings: ["Something to know"],
        destructive: true,
        transactional: true,
        statementOffset: 0,
      }),
    });
    renderDetails();
    await startEditing();
    await userEvent.click(screen.getByRole("tab", { name: "Indexes" }));
    const row = screen.getByRole("row", { name: "Index orders_total_idx" });
    await userEvent.click(within(row).getByRole("button", { name: "Drop" }));
    expect(row.dataset.state).toBe("dropped");

    await userEvent.click(screen.getByRole("button", { name: "Preview and apply" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Something to know")).toBeDefined();
    expect(within(dialog).getByText(/Data in a dropped column is lost/)).toBeDefined();
    await userEvent.click(within(dialog).getByRole("button", { name: "Apply and drop" }));
    await waitFor(() => {
      expect(applied[0]?.confirmed).toBe(true);
    });
  });

  it("keeps the changes and names the failed statement when Apply fails", async () => {
    install({
      schema_edit_preview: () => ({
        statements: ["PRAGMA foreign_keys = OFF", "ALTER 1", "ALTER 2"],
        warnings: [],
        destructive: false,
        transactional: true,
        statementOffset: 1,
      }),
      schema_edit_apply: () => {
        throw { code: "execution", message: "column exists", detail: { statement: 1, applied: 0 } };
      },
    });
    renderDetails();
    await startEditing();
    const row = screen.getByRole("row", { name: "Column total" });
    await userEvent.click(within(row).getByLabelText("Not null"));
    await userEvent.click(screen.getByRole("button", { name: "Preview and apply" }));
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Apply" }),
    );

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("column exists")).toBeDefined();
    expect(within(alert).getByText("ALTER 2")).toBeDefined();
    expect(screen.getByText("1 change not applied")).toBeDefined();
  });

  it("lists what stayed applied when a plan without a transaction fails part-way", async () => {
    install({
      schema_edit_preview: () => ({
        statements: ["ALTER 1", "ALTER 2"],
        warnings: [],
        destructive: false,
        transactional: false,
        statementOffset: 0,
      }),
      schema_edit_apply: () => {
        throw { code: "execution", message: "duplicate key", detail: { statement: 1, applied: 1 } };
      },
    });
    renderDetails();
    await startEditing();
    await userEvent.type(screen.getByLabelText("Table comment"), "Orders");
    await userEvent.click(screen.getByRole("button", { name: "Preview and apply" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/one at a time/)).toBeDefined();
    await userEvent.click(within(dialog).getByRole("button", { name: "Apply" }));

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(/stayed applied/)).toBeDefined();
    expect(within(alert).getByText("ALTER 1")).toBeDefined();
    // The table changed, so the staged changes no longer apply.
    expect(useEdits.getState().pending["table-1"]).toBeUndefined();
  });

  it("adds an index from the form", async () => {
    install();
    renderDetails();
    await startEditing();
    await userEvent.click(screen.getByRole("tab", { name: "Indexes" }));
    await userEvent.click(screen.getByRole("button", { name: "Add index" }));
    const form = screen.getByRole("form", { name: "New index" });
    const add = within(form).getByRole("button", { name: "Add" });
    expect(add).toHaveProperty("disabled", true);
    await userEvent.type(within(form).getByLabelText("Name"), "orders_both");
    await userEvent.type(within(form).getByLabelText("Columns"), "id, lower(total)");
    await userEvent.click(add);

    expect(screen.getByRole("row", { name: "Index orders_both" }).dataset.state).toBe("added");
    expect(useSchemaEdits.getState().pending["table-1"]?.addedObjects).toEqual([
      {
        kind: "index",
        value: {
          name: "orders_both",
          columns: ["id", "lower(total)"],
          unique: false,
          method: null,
          predicate: null,
        },
      },
    ]);
  });

  it("discards the staged changes", async () => {
    install();
    renderDetails();
    await startEditing();
    const row = screen.getByRole("row", { name: "Column total" });
    await userEvent.click(within(row).getByRole("button", { name: "Drop" }));
    await userEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(useEdits.getState().pending["table-1"]).toBeUndefined();
    expect(screen.getByRole("row", { name: "Column total" }).dataset.state).toBe("unchanged");
  });
});
