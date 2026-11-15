import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Inputs } from "./edits";
import { type EditRequest, ValueEditor } from "./ValueEditor";

function show(keyType: string, inputs: Inputs) {
  const onEdit = vi.fn<(request: EditRequest) => void>();
  const onInvalid = vi.fn();
  const onInputs = vi.fn();
  const view = render(
    <ValueEditor
      keyType={keyType}
      inputs={inputs}
      onInputs={onInputs}
      onEdit={onEdit}
      onInvalid={onInvalid}
    />,
  );
  return { onEdit, onInvalid, onInputs, ...view };
}

describe("ValueEditor", () => {
  it("offers nothing for types edited only in the console", () => {
    const { container } = show("stream", {});
    expect(container).toBeEmptyDOMElement();
  });

  it("edits lists by index, at either end, or by value", async () => {
    const { onEdit, onInvalid } = show("list", { index: "2", value: "x" });

    await userEvent.click(screen.getByRole("button", { name: "Set at index" }));
    await userEvent.click(screen.getByRole("button", { name: "Push to head" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove value" }));

    expect(onEdit.mock.calls.map(([request]) => request)).toEqual([
      { edit: { kind: "list_set", index: 2, value: "x" }, removes: false },
      { edit: { kind: "list_push", value: "x", head: true }, removes: false },
      { edit: { kind: "list_remove", value: "x" }, removes: true },
    ]);
    expect(onInvalid).not.toHaveBeenCalled();
  });

  it("reports an input that does not make an edit", async () => {
    const { onEdit, onInvalid } = show("list", { index: "two", value: "x" });

    await userEvent.click(screen.getByRole("button", { name: "Set at index" }));

    expect(onInvalid).toHaveBeenCalledWith("The index must be a whole number.");
    expect(onEdit).not.toHaveBeenCalled();
  });

  it("edits sets and sorted sets by member", async () => {
    const set = show("set", { member: "m" });
    await userEvent.click(screen.getByRole("button", { name: "Remove member" }));
    expect(set.onEdit).toHaveBeenCalledWith({
      edit: { kind: "set_remove", member: "m" },
      removes: true,
    });
    set.unmount();

    const zset = show("zset", { member: "m", score: "1.5" });
    await userEvent.click(screen.getByRole("button", { name: "Add or rescore" }));
    expect(zset.onEdit).toHaveBeenCalledWith({
      edit: { kind: "zset_add", member: "m", score: 1.5 },
      removes: false,
    });
  });

  it("types into the inputs, a text area for strings", () => {
    const string = show("string", { value: "old" });
    fireEvent.change(screen.getByLabelText("Value"), { target: { value: "new" } });
    expect(string.onInputs).toHaveBeenCalledWith({ value: "new" });
    expect(screen.getByLabelText("Value").tagName).toBe("TEXTAREA");
    string.unmount();

    const hash = show("hash", { field: "a" });
    fireEvent.change(screen.getByLabelText("Field"), { target: { value: "b" } });
    expect(hash.onInputs).toHaveBeenCalledWith({ field: "b" });
  });
});
