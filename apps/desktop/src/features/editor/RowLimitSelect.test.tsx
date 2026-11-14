import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { RowLimitSelect } from "./RowLimitSelect";

describe("RowLimitSelect", () => {
  it("picks a preset or a valid custom number", async () => {
    const onChange = vi.fn();
    render(<RowLimitSelect value={1000} onChange={onChange} />);

    await userEvent.selectOptions(screen.getByLabelText("Row limit"), "100");
    expect(onChange).toHaveBeenLastCalledWith(100);

    await userEvent.selectOptions(screen.getByLabelText("Row limit"), "custom");
    const custom = screen.getByLabelText("Custom row limit");
    await userEvent.clear(custom);
    await userEvent.type(custom, "250{Enter}");
    expect(onChange).toHaveBeenLastCalledWith(250);

    await userEvent.clear(custom);
    await userEvent.type(custom, "0");
    await userEvent.tab();
    expect(onChange).toHaveBeenCalledTimes(2);
    expect(custom).toHaveValue(1000);
  });

  it("shows a non-preset value as custom", () => {
    render(<RowLimitSelect value={42} onChange={vi.fn()} />);

    expect(screen.getByLabelText("Custom row limit")).toHaveValue(42);
  });
});
