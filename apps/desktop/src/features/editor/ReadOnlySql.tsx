import { EditorView, basicSetup } from "codemirror";
import { EditorState } from "@codemirror/state";
import { useEffect, useRef } from "react";

import { editorTheme } from "./theme";

/** SQL shown in CodeMirror that cannot be edited (DDL). Remount via `key` to change it. */
export function ReadOnlySql({ text, label }: { text: string; label: string }) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!host.current) return;
    const view = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: text,
        extensions: [
          basicSetup,
          editorTheme,
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          EditorView.contentAttributes.of({ "aria-label": label }),
        ],
      }),
    });
    return () => {
      view.destroy();
    };
  }, [text, label]);

  return <div ref={host} className="h-full overflow-hidden" />;
}
