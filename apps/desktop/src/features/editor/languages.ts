import { json } from "@codemirror/lang-json";
import { sql } from "@codemirror/lang-sql";
import { xml } from "@codemirror/lang-xml";
import type { Extension } from "@codemirror/state";

export type CodeLanguage = "sql" | "json" | "xml" | "text";

/** The CodeMirror language extension for each highlighted language. */
export const LANGUAGES: Record<CodeLanguage, () => Extension> = {
  sql: () => sql(),
  json: () => json(),
  xml: () => xml(),
  text: () => [],
};
