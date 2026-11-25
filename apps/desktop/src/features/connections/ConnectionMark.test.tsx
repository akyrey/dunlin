import { render, screen } from "@testing-library/react";

import { ConnectionMark } from "./ConnectionMark";

describe("ConnectionMark", () => {
  it("draws the connection's colour dot and its driver's logo", () => {
    const { container } = render(<ConnectionMark driver="postgres" color="#ff0000" />);

    expect(screen.getByRole("img", { name: "PostgreSQL" })).toBeInTheDocument();
    const dot = container.querySelector("[data-connection-color]");
    expect(dot).toHaveStyle({ backgroundColor: "#ff0000" });
  });

  it("keeps the dot's space without a colour so names stay aligned", () => {
    const { container } = render(<ConnectionMark driver="sqlite" color={null} />);

    const dot = container.querySelector<HTMLElement>("[data-connection-color]");
    expect(dot?.style.backgroundColor).toBe("transparent");
  });
});
