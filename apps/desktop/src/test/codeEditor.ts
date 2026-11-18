import { act } from "@testing-library/react";
import { EditorView } from "codemirror";

/** Replaces the whole text of the CodeMirror editor that holds `element`, as typing would. jsdom
 * cannot type into CodeMirror's contenteditable, so the change is dispatched directly. */
export function replaceEditorText(element: HTMLElement, text: string): void {
  const view = EditorView.findFromDOM(element);
  if (!view) throw new Error("The element is not inside a CodeMirror editor");
  act(() => {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
  });
}
