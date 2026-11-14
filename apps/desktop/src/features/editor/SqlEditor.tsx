import { EditorView, basicSetup } from "codemirror";
import { EditorSelection, EditorState, Prec } from "@codemirror/state";
import { keymap } from "@codemirror/view";
import { type RefObject, useEffect, useImperativeHandle, useRef } from "react";

import type { Selection } from "./editorStore";

export type EditorHandle = {
  selection: () => Selection;
  focusAt: (offset: number) => void;
};

type Props = {
  initialText: string;
  onChange: (text: string) => void;
  onRun: (selection: Selection) => void;
  handleRef: RefObject<EditorHandle | null>;
};

const theme = EditorView.theme(
  {
    "&": { height: "100%", fontSize: "13px", backgroundColor: "#18181b", color: "#e4e4e7" },
    ".cm-content": {
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
      caretColor: "#e4e4e7",
    },
    ".cm-gutters": { backgroundColor: "#18181b", color: "#71717a", border: "none" },
    ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "#27272a80" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "#0369a180",
    },
  },
  { dark: true },
);

function selectionOf(view: EditorView): Selection {
  const main = view.state.selection.main;
  return { from: main.from, to: main.to, head: main.head };
}

/** A plain CodeMirror editor. Highlighting per dialect and autocomplete arrive in Phase 5. */
export function SqlEditor({ initialText, onChange, onRun, handleRef }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const callbacks = useRef({ onChange, onRun });

  useEffect(() => {
    callbacks.current = { onChange, onRun };
  });

  useImperativeHandle(handleRef, () => ({
    selection: () => (view.current ? selectionOf(view.current) : { from: 0, to: 0, head: 0 }),
    focusAt: (offset) => {
      const current = view.current;
      if (!current) return;
      const position = Math.min(offset, current.state.doc.length);
      current.dispatch({ selection: EditorSelection.cursor(position), scrollIntoView: true });
      current.focus();
    },
  }));

  useEffect(() => {
    if (!host.current) return;
    const runKey = Prec.highest(
      keymap.of([
        {
          key: "Mod-Enter",
          run: (target) => {
            callbacks.current.onRun(selectionOf(target));
            return true;
          },
        },
      ]),
    );
    const created = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: initialText,
        extensions: [
          basicSetup,
          runKey,
          theme,
          EditorView.contentAttributes.of({ "aria-label": "SQL editor" }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) callbacks.current.onChange(update.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = created;
    return () => {
      created.destroy();
      view.current = null;
    };
    // The editor owns its document after mount; remount (via `key`) to replace it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div ref={host} className="h-full overflow-hidden" />;
}
