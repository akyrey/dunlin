import { render, screen, within } from "@testing-library/react";

import type { TableStats } from "../../ipc";
import { TableStatsView } from "./TableStatsView";

const empty: TableStats = {
  totalBytes: null,
  tableBytes: null,
  indexBytes: null,
  estimatedRows: null,
  indexes: [],
  deadRows: null,
  lastVacuum: null,
  lastAnalyze: null,
  updatedAt: null,
  countedSince: null,
};

describe("TableStatsView", () => {
  it("shows sizes, counts and unused indexes", () => {
    render(
      <TableStatsView
        stats={{
          ...empty,
          totalBytes: 3 * 1024 * 1024,
          estimatedRows: 12_345,
          lastAnalyze: "2026-11-20",
          countedSince: "2026-11-01",
          indexes: [
            { name: "a_pkey", bytes: 2048, scans: 7 },
            { name: "a_b_idx", bytes: null, scans: 0 },
          ],
        }}
      />,
    );
    expect(screen.getByText("3.0 MiB")).toBeInTheDocument();
    expect(screen.getByText("12,345")).toBeInTheDocument();
    expect(screen.getByText("2026-11-20")).toBeInTheDocument();
    expect(screen.queryByText("Dead rows")).toBeNull();
    const unused = screen.getByRole("row", { name: "Index a_b_idx" });
    expect(within(unused).getByText("unused")).toBeInTheDocument();
    expect(
      within(screen.getByRole("row", { name: "Index a_pkey" })).queryByText("unused"),
    ).toBeNull();
    expect(screen.getByText(/since 2026-11-01/)).toBeInTheDocument();
  });

  it("says so when the database reports nothing, and hides scans it does not count", () => {
    render(
      <TableStatsView stats={{ ...empty, indexes: [{ name: "i", bytes: null, scans: null }] }} />,
    );
    expect(screen.getByText(/reports no sizes or counts/)).toBeInTheDocument();
    expect(screen.queryByText("Scans")).toBeNull();
    expect(screen.queryByText("unused")).toBeNull();
  });
});
