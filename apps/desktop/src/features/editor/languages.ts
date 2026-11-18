import { sql } from "@codemirror/lang-sql";
import type { LanguageSet } from "@roost/editor";

/** Dunlin's languages on top of the kit's built-in ones. One object, so editors are not rebuilt. */
export const LANGUAGES: LanguageSet = { sql: () => sql() };

export type CodeLanguage = "sql" | "json" | "xml" | "text";
