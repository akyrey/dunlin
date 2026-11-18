/** What the WHERE filter bar completes and how it stays one line. Pure CodeMirror pieces, kept
 * apart from the component so they can be tested without a view. */
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { syntaxTree } from "@codemirror/language";
import { EditorState, type Extension } from "@codemirror/state";

/** Syntax nodes where a column name makes no sense. */
const NO_COLUMNS_IN = new Set(["String", "LineComment", "BlockComment", "QuotedIdentifier"]);

/** Newlines typed or pasted into the filter become single spaces. */
export const singleLine: Extension = EditorState.transactionFilter.of((tr) => {
  if (!tr.docChanged || tr.newDoc.lines === 1) return tr;
  const flat = tr.newDoc.toString().replace(/[ \t]*[\r\n]+[ \t]*/g, " ");
  return [tr, { changes: { from: 0, to: tr.newDoc.length, insert: flat }, sequential: true }];
});

/** The filtered table's own columns, unqualified, ahead of everything else. */
export function columnSource(columns: () => readonly string[]) {
  return (context: CompletionContext): CompletionResult | null => {
    const word = context.matchBefore(/\w*/);
    if (!word) return null;
    if (word.from === word.to && !context.explicit) return null;
    if (context.state.sliceDoc(word.from - 1, word.from) === ".") return null;
    const node = syntaxTree(context.state).resolveInner(context.pos, -1);
    if (NO_COLUMNS_IN.has(node.name)) return null;
    const options: Completion[] = columns().map((label) => ({ label, type: "property", boost: 1 }));
    return { from: word.from, options, validFor: /^\w*$/ };
  };
}

/** Earlier filters for this table that start with what is typed, replacing the whole text. */
export function recentSource(recent: () => readonly string[]) {
  return (context: CompletionContext): CompletionResult | null => {
    const typed = context.state.sliceDoc(0, context.pos);
    if (context.pos !== context.state.doc.length) return null;
    const lower = typed.toLowerCase();
    const options: Completion[] = recent()
      .filter((f) => f !== typed && f.toLowerCase().startsWith(lower))
      .map((label) => ({ label, type: "text", section: "Recent", boost: -1 }));
    if (options.length === 0) return null;
    return { from: 0, options, filter: false };
  };
}
