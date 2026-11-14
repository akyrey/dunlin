import { act, render, screen } from "@testing-library/react";

import { ElapsedTimer } from "./ElapsedTimer";

describe("ElapsedTimer", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("counts up from the start time", () => {
    vi.setSystemTime(10_000);
    render(<ElapsedTimer startedAt={10_000} />);
    expect(screen.getByRole("timer")).toHaveTextContent("0.0 s");

    act(() => {
      vi.advanceTimersByTime(1_500);
    });

    expect(screen.getByRole("timer")).toHaveTextContent("1.5 s");
  });
});
