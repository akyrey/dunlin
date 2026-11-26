import { type SQLNamespace, sql } from "@codemirror/lang-sql";
import {
  Compartment,
  EditorSelection,
  EditorState,
  Prec,
  StateEffect,
  StateField,
} from "@codemirror/state";
import { Decoration, type DecorationSet, keymap } from "@codemirror/view";
import { EditorView, basicSetup } from "codemirror";
import { type RefObject, useEffect, useImperativeHandle, useRef } from "react";

import type { DriverKind } from "../../ipc";
import { type Span, statementAt } from "../../lib/text";
import { dialectFor } from "./dialects";
import { formatSql } from "./formatSql";
import { redisLanguage } from "./redisLanguage";
import type { RunMode, Selection } from "./run";
import { bindingOf } from "../shortcuts/registry";
import { useEditorAppearance } from "@roost/editor";

export type EditorHandle = {
  selection: () => Selection;
  focusAt: (offset: number) => void;
  focus: () => void;
  /** Formats the selection, or the whole text; a failure goes to `onFormatError`. */
  format: () => void;
};

export type Completion = { schema: SQLNamespace; defaultSchema: string | undefined };

type Props = {
  initialText: string;
  driver: DriverKind;
  completion: Completion | undefined;
  /** Statements of the current text, once split; the one at the cursor is highlighted. */
  ranges: Span[] | null;
  /** The failing statement, or the reported error position, underlined. */
  errorRange: Span | null;
  running: boolean;
  onChange: (text: string) => void;
  onRun: (selection: Selection, mode: RunMode) => void;
  onExplain?: (selection: Selection) => void;
  onCancel: () => void;
  onSave: () => void;
  onFormatError: (message: string) => void;
  handleRef: RefObject<EditorHandle | null>;
};

type Marks = { ranges: Span[]; error: Span | null };

const setMarks = StateEffect.define<Marks>();

/** Statement ranges and the error, cleared as soon as the text changes (they would be stale). */
const marksField = StateField.define<Marks>({
  create: () => ({ ranges: [], error: null }),
  update: (value, transaction) => {
    for (const effect of transaction.effects) if (effect.is(setMarks)) return effect.value;
    return transaction.docChanged ? { ranges: [], error: null } : value;
  },
});

const activeStatement = Decoration.mark({ class: "cm-activeStatement" });
const statementError = Decoration.mark({ class: "cm-statementError" });

const marksDecorations = EditorView.decorations.compute(
  [marksField, "selection"],
  (state): DecorationSet => {
    const { ranges, error } = state.field(marksField);
    const length = state.doc.length;
    const marks = [];
    const active = statementAt(ranges, state.selection.main.head);
    if (active && active.end <= length && active.start < active.end) {
      marks.push(activeStatement.range(active.start, active.end));
    }
    if (error && error.end <= length && error.start < error.end) {
      marks.push(statementError.range(error.start, error.end));
    }
    return Decoration.set(marks, true);
  },
);

function selectionOf(view: EditorView): Selection {
  const main = view.state.selection.main;
  return { from: main.from, to: main.to, head: main.head };
}

/** Replaces the selection, or the whole text, with its formatted SQL as one undoable change.
 * Returns the error when the SQL cannot be formatted. */
function formatIn(view: EditorView, driver: DriverKind): string | null {
  const main = view.state.selection.main;
  const whole = main.empty;
  const from = whole ? 0 : main.from;
  const to = whole ? view.state.doc.length : main.to;
  const original = view.state.sliceDoc(from, to);
  const result = formatSql(original, driver);
  if (!result.ok) return result.error;
  if (result.text === original) return null;
  const end = from + result.text.length;
  view.dispatch({
    changes: { from, to, insert: result.text },
    selection: whole
      ? EditorSelection.cursor(Math.min(main.head, end))
      : EditorSelection.range(from, end),
    userEvent: "input.format",
    scrollIntoView: true,
  });
  return null;
}

function formatAndReport(view: EditorView, props: Pick<Props, "driver" | "onFormatError">) {
  const error = formatIn(view, props.driver);
  if (error) props.onFormatError(error);
}

function language(driver: DriverKind, completion: Completion | undefined) {
  const dialect = dialectFor(driver);
  if (!dialect) return redisLanguage();
  return sql({
    dialect,
    schema: completion?.schema,
    defaultSchema: completion?.defaultSchema,
    upperCaseKeywords: true,
  });
}

/** The SQL editor: dialect highlighting, schema autocomplete, statement under the cursor. */
export function SqlEditor(props: Props) {
  const { initialText, driver, completion, ranges, errorRange, handleRef } = props;
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

  useImperativeHandle(handleRef, () => ({
    selection: () => (view.current ? selectionOf(view.current) : { from: 0, to: 0, head: 0 }),
    focusAt: (offset) => {
      const current = view.current;
      if (!current) return;
      const position = Math.min(offset, current.state.doc.length);
      current.dispatch({ selection: EditorSelection.cursor(position), scrollIntoView: true });
      current.focus();
    },
    focus: () => view.current?.focus(),
    format: () => {
      const current = view.current;
      if (!current) return;
      formatAndReport(current, callbacks.current);
      current.focus();
    },
  }));

  useEffect(() => {
    if (!host.current) return;
    const run = (mode: RunMode) => (target: EditorView) => {
      callbacks.current.onRun(selectionOf(target), mode);
      return true;
    };
    const runKeys = Prec.highest(
      keymap.of([
        { key: bindingOf("run-statement"), run: run("statement") },
        { key: bindingOf("run-script"), run: run("script") },
        {
          key: bindingOf("explain"),
          run: (target) => {
            callbacks.current.onExplain?.(selectionOf(target));
            return true;
          },
        },
        {
          key: bindingOf("format-sql"),
          run: (target) => {
            formatAndReport(target, callbacks.current);
            return true;
          },
        },
        {
          key: bindingOf("save-query"),
          run: () => {
            callbacks.current.onSave();
            return true;
          },
        },
      ]),
    );
    // Default precedence, so Escape first closes an open autocomplete list.
    const cancelKey = keymap.of([
      {
        key: bindingOf("cancel"),
        run: () => {
          if (!callbacks.current.running) return false;
          callbacks.current.onCancel();
          return true;
        },
      },
    ]);
    const created = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: initialText,
        extensions: [
          basicSetup,
          runKeys,
          cancelKey,
          themeSlot.current.of(initialAppearance.current),
          languageSlot.current.of(language(callbacks.current.driver, callbacks.current.completion)),
          marksField,
          marksDecorations,
          EditorView.contentAttributes.of({
            "aria-label": "SQL editor",
            // No smart quotes, dashes or spelling fixes in SQL.
            autocorrect: "off",
            autocapitalize: "off",
            spellcheck: "false",
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
    // The editor owns its document after mount; remount (via `key`) to replace it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    view.current?.dispatch({ effects: themeSlot.current.reconfigure(appearance) });
  }, [appearance]);

  // A refreshed schema (after DDL or a refresh) reconfigures completion in place.
  useEffect(() => {
    view.current?.dispatch({
      effects: languageSlot.current.reconfigure(language(driver, completion)),
    });
  }, [driver, completion]);

  useEffect(() => {
    view.current?.dispatch({ effects: setMarks.of({ ranges: ranges ?? [], error: errorRange }) });
  }, [ranges, errorRange]);

  return <div ref={host} className="h-full overflow-hidden" />;
}
