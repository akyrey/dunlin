import { render, screen } from "@testing-library/react";

import { sessionFor, sqliteProfile } from "../test/fixtures";
import { StatusBar } from "./StatusBar";

describe("StatusBar", () => {
  it("says when nothing is open", () => {
    render(<StatusBar profile={null} session={null} result={null} />);

    expect(screen.getByText("No connection open")).toBeInTheDocument();
  });

  it("shows the connection, read-only badge and result summary", () => {
    const profile = sqliteProfile({ readOnly: true });
    const result = {
      outcome: { kind: "rows" as const, columns: [], rows: [[]], truncated: true },
      elapsedMs: 4,
      statementClass: "read" as const,
    };

    render(<StatusBar profile={profile} session={sessionFor(profile)} result={result} />);

    expect(screen.getByText("Local")).toBeInTheDocument();
    expect(screen.getByText("read-only")).toBeInTheDocument();
    expect(screen.getByText("1+ row · 4 ms")).toBeInTheDocument();
  });

  it("summarises affected rows", () => {
    const result = {
      outcome: { kind: "affected" as const, rows: "3" },
      elapsedMs: 1,
      statementClass: "write" as const,
    };

    render(<StatusBar profile={sqliteProfile()} session={null} result={result} />);

    expect(screen.getByText("3 affected · 1 ms")).toBeInTheDocument();
  });
});
