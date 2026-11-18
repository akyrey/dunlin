import { render, screen, waitFor } from "@testing-library/react";

import { FilterEditor, type FilterEditorProps } from "./FilterEditor";

function show(overrides: Partial<FilterEditorProps> = {}) {
  const props: FilterEditorProps = {
    value: "",
    onChange: vi.fn(),
    onSubmit: vi.fn(),
    driver: "postgres",
    completion: undefined,
    columns: ["id", "kind"],
    recent: [],
    placeholder: "a filter",
    label: "Filter",
    ...overrides,
  };
  const view = render(<FilterEditor {...props} />);
  return { props, ...view };
}

const content = () => screen.getByRole("textbox", { name: "Filter" });

describe("FilterEditor", () => {
  it("turns off spelling, capitals and smart punctuation", () => {
    show();
    expect(content()).toHaveAttribute("spellcheck", "false");
    expect(content()).toHaveAttribute("autocorrect", "off");
    expect(content()).toHaveAttribute("autocapitalize", "off");
  });

  it("follows a value set from outside", async () => {
    const { props, rerender } = show({ value: "kind = 1" });
    expect(content()).toHaveTextContent("kind = 1");
    rerender(<FilterEditor {...props} value="" />);
    await waitFor(() => {
      expect(content()).not.toHaveTextContent("kind");
    });
  });

  it("submits the text on Enter", () => {
    const { props } = show({ value: "id > 3" });
    content().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(props.onSubmit).toHaveBeenCalledWith("id > 3");
  });
});
