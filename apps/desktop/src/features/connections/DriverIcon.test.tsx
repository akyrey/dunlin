import { render, screen } from "@testing-library/react";

import type { DriverKind } from "../../ipc";
import { DriverIcon } from "./DriverIcon";

describe("DriverIcon", () => {
  it.each<[DriverKind, string]>([
    ["postgres", "PostgreSQL"],
    ["mysql", "MySQL / MariaDB"],
    ["sqlite", "SQLite"],
    ["redis", "Redis / Valkey"],
  ])("draws a labelled logo for %s", (driver, label) => {
    render(<DriverIcon driver={driver} />);

    const icon = screen.getByRole("img", { name: label });
    expect(icon.querySelector("path")?.getAttribute("d")).toBeTruthy();
  });

  it("draws a distinct logo per driver", () => {
    const drivers: DriverKind[] = ["postgres", "mysql", "sqlite", "redis"];
    const paths = drivers.map((driver) => {
      const { container, unmount } = render(<DriverIcon driver={driver} />);
      const d = container.querySelector("path")?.getAttribute("d");
      unmount();
      return d;
    });

    expect(new Set(paths).size).toBe(drivers.length);
  });
});
