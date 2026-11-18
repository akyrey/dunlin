/** The WHERE filter bar: one line of SQL with the editor's highlighting and autocomplete. */
import { autocompletion, closeBrackets, startCompletion } from "@codemirror/autocomplete";
import { sql } from "@codemirror/lang-sql";
import { Compartment, EditorSelection, EditorState, Prec } from "@codemirror/state";
import { EditorView, keymap, placeholder as placeholderText } from "@codemirror/view";
import { minimalSetup } from "codemirror";
import { useEffect, useRef } from "react";

import type { DriverKind } from "../../ipc";
import { useEditorAppearance } from "@roost/editor";
import { dialectFor } from "../editor/dialects";
import type { Completion } from "../editor/SqlEditor";
import { columnSource, recentSource, singleLine } from "./filterCompletion";

export type FilterEditorProps = {
  value: string;
  onChange: (text: string) => void;
  onSubmit: (text: string) => void;
  driver: DriverKind;
  completion: Completion | undefined;
  /** The filtered table's columns, completed unqualified. */
  columns: readonly string[];
  /** Earlier filters for this table, newest first. */
  recent: readonly string[];
  placeholder: string;
  label: string;
};

/** Sits inside an `.input` box, so it drops the editor's own frame. */
const oneLineTheme = Prec.highest(
  EditorView.theme({
    "&": { height: "auto", backgroundColor: "transparent", fontSize: "inherit" },
    "&.cm-focused": { outline: "none" },
    ".cm-content": { padding: "0" },
    ".cm-line": { padding: "0" },
    ".cm-scroller": { overflow: "hidden" },
  }),
);

type Sources = { columns: readonly string[]; recent: readonly string[] };

function language(driver: DriverKind, completion: Completion | undefined, sources: () => Sources) {
  const support = sql({
    dialect: dialectFor(driver) ?? undefined,
    schema: completion?.schema,
    defaultSchema: completion?.defaultSchema,
    upperCaseKeywords: true,
  });
  return [
    support,
    support.language.data.of({ autocomplete: columnSource(() => sources().columns) }),
    support.language.data.of({ autocomplete: recentSource(() => sources().recent) }),
  ];
}

export function FilterEditor(props: FilterEditorProps) {
  const { value, driver, completion } = props;
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const languageSlot = useRef(new Compartment());
  const themeSlot = useRef(new Compartment());
  const appearance = useEditorAppearance();
  const initialAppearance = useRef(appearance);
  const callbacks = useRef(props);

  useEffect(() => {
    callbacks.current = props;
  });

  useEffect(() => {
    if (!host.current) return;
    const sources = () => callbacks.current;
    const submit = Prec.high(
      keymap.of([
        {
          key: "Enter",
          run: (target) => {
            callbacks.current.onSubmit(target.state.doc.toString());
            return true;
          },
        },
      ]),
    );
    const created = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: callbacks.current.value,
        extensions: [
          minimalSetup,
          submit,
          singleLine,
          closeBrackets(),
          autocompletion(),
          placeholderText(callbacks.current.placeholder),
          themeSlot.current.of(initialAppearance.current),
          oneLineTheme,
          languageSlot.current.of(
            language(callbacks.current.driver, callbacks.current.completion, sources),
          ),
          EditorView.contentAttributes.of({
            "aria-label": callbacks.current.label,
            role: "textbox",
            "aria-multiline": "false",
            // No smart quotes, dashes or spelling fixes in SQL.
            autocorrect: "off",
            autocapitalize: "off",
            spellcheck: "false",
          }),
          EditorView.domEventHandlers({
            // An empty filter offers the recent ones, as the old suggestion list did.
            focus: (_event, target) => {
              if (target.state.doc.length === 0 && callbacks.current.recent.length > 0) {
                startCompletion(target);
              }
              return false;
            },
          }),
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
    // Created once; later prop changes reconfigure it below.
  }, []);

  useEffect(() => {
    view.current?.dispatch({ effects: themeSlot.current.reconfigure(appearance) });
  }, [appearance]);

  useEffect(() => {
    view.current?.dispatch({
      effects: languageSlot.current.reconfigure(
        language(driver, completion, () => callbacks.current),
      ),
    });
  }, [driver, completion]);

  // Cleared or straightened from outside: replace the text, cursor at the end.
  useEffect(() => {
    const current = view.current;
    if (!current || current.state.doc.toString() === value) return;
    current.dispatch({
      changes: { from: 0, to: current.state.doc.length, insert: value },
      selection: EditorSelection.cursor(value.length),
    });
  }, [value]);

  return <div ref={host} className="min-w-0 flex-1" />;
}
